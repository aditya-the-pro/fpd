use reqwest::header::{HeaderMap, ACCEPT, USER_AGENT};
use serde_json::Value;
use std::env;
use std::env::current_dir;
use std::fs::{create_dir, File};
use std::io::copy;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
extern crate sanitize_filename;

const API_BASE_URL: &'static str = "https://pipedapi.kavin.rocks/";
const MAX_THREAD_AT_ONCE: usize = 4;

fn main() {
    let args: Vec<String> = env::args().collect();
    let playlist_match_str_ref = "https://www.youtube.com/playlist?list=";
    let video_match_str_ref = "https://www.youtube.com/watch?v=";
    let input_link: String;
    let id: String;
    let link_type: String;
    let dl_msg = "[INFO] starting the download in the current dir";

    match args.get(1) {
        Some(arg) => {
            input_link = arg.to_string();
        }
        None => {
            println!("[ERR] no link passed as arg");
            std::process::exit(69);
        }
    }

    if input_link.contains(playlist_match_str_ref) {
        link_type = "playlist".to_string();
        id = input_link
            .trim_start_matches(playlist_match_str_ref)
            .to_string();
    } else if input_link.contains(video_match_str_ref) {
        link_type = "video".to_string();
        id = input_link
            .trim_start_matches(video_match_str_ref)
            .to_string();
    } else {
        println!("[ERR] err while identifying link");
        std::process::exit(69);
    }

    if link_type == "playlist" {
        let client = reqwest::blocking::Client::new();

        let response = client
            .get(format!("{}playlists/{}", API_BASE_URL, id))
            .headers(req_headers())
            .send();

        let data = response.expect("failed to make http request to get playlist metadata");

        match data.text() {
            Ok(data) => {
                let v: Value = serde_json::from_str(&data).expect("failed to parse json");
                match v["relatedStreams"].as_array() {
                    Some(json) => {
                        let total_vids = v["videos"].to_string();
                        println!("[INFO] found playlist :---> {}", v["name"]);
                        println!("[INFO] no. of videos in playlist :---> {}", total_vids);
                        println!("{}", dl_msg);

                        let mut video_arr: Vec<String> = vec![];

                        for single_video in json.iter() {
                            let video_id = single_video["url"]
                                .to_string()
                                .trim_matches('"')
                                .trim_start_matches("/watch?v=")
                                .to_string();
                            video_arr.push(video_id);
                        }

                        let cwd = current_dir().unwrap();
                        let dir_path = cwd.join(sanitize_filename::sanitize(v["name"].to_string()));

                        if dir_path.exists() {
                            println!("[ERR] dir with same name as playlist exists");
                            println!("[INFO] exiting");
                            std::process::exit(69);
                        } else {
                            create_dir(dir_path.clone()).unwrap();

                            let video_download_counter = Arc::new(Mutex::new(0));
                            let total_vids = Arc::new(total_vids);

                            for relay in video_arr.chunks(MAX_THREAD_AT_ONCE) {
                                let mut t_handles = vec![];
                                for id in relay {
                                    let t_id = id.clone();
                                    let t_dir_path =
                                        dir_path.to_str().unwrap().parse::<String>().unwrap();
                                    let video_dl_counter = video_download_counter.clone();
                                    let t_total_vids = total_vids.clone();
                                    let handle = thread::spawn(move || {
                                        let client = reqwest::blocking::Client::new();

                                        let response = client
                                            .get(format!("{}streams/{}", API_BASE_URL, t_id))
                                            .headers(req_headers())
                                            .send()
                                            .unwrap();

                                        let json_data = response.text().unwrap();
                                        let v: Value = serde_json::from_str(&json_data)
                                            .expect("failed to parse json");

                                        let file_url = v["videoStreams"]
                                            .as_array()
                                            .unwrap()
                                            .iter()
                                            .last()
                                            .unwrap();

                                        let mut video_bits = client
                                            .get(file_url["url"].to_string().trim_matches('"'))
                                            .headers(req_headers())
                                            .send()
                                            .unwrap();

                                        let filename =
                                            sanitize_filename::sanitize(v["title"].to_string());

                                        if video_bits.status().is_success() {
                                            let t_dir_path: PathBuf =
                                                t_dir_path.parse::<PathBuf>().unwrap();

                                            let mut file = File::create(
                                                t_dir_path.join(format!("{}.mp4", filename)),
                                            )
                                            .unwrap();

                                            match copy(&mut video_bits, &mut file) {
                                                Ok(_) => {
                                                    let mut dl_stats = video_dl_counter.lock().unwrap();
                                                    *dl_stats +=1;
                                                    println!("[{dl_stats}/{}] download of {} finished successfully",t_total_vids,v["title"])
                                                }
                                                Err(err) => println!(
                                                    "[ERR] error while writing the downloaded file: {}",
                                                    err
                                                ),
                                            }
                                        }
                                    });

                                    t_handles.push(handle);
                                }

                                for h in t_handles.into_iter() {
                                    h.join().unwrap();
                                }
                            }

                            println!("[INFO] playlist donwloaded successfully")
                        }
                    }
                    None => println!("[ERR] unable to find playlist from the given link"),
                }
            }
            Err(err) => println!("failed due to err: {}", err),
        }
    } else if link_type == "video" {
        let client = reqwest::blocking::Client::new();

        let response = client
            .get(format!("{}streams/{}", API_BASE_URL, id))
            .headers(req_headers())
            .send();

        let data = response.expect("failed to make http request to get video metadata");
        match data.text() {
            Ok(data) => {
                let v: Value = serde_json::from_str(&data).expect("failed to parse json");
                match v["videoStreams"].as_array() {
                    Some(json) => {
                        let video_link = json.last();
                        println!("[INFO] found video :---> {}", v["title"]);
                        println!("{}", dl_msg);

                        match video_link {
                            None => println!("failed to get the video link to download"),
                            Some(json_data) => {
                                let mut vid_data = client
                                    .get(json_data["url"].to_string().trim_matches('"'))
                                    .headers(req_headers())
                                    .send()
                                    .unwrap();

                                let cwd =
                                    current_dir().expect("Failed to get current working directory");

                                let video_path = cwd.join(format!(
                                    "{}.mp4",
                                    sanitize_filename::sanitize(v["title"].to_string())
                                ));

                                if video_path.exists() {
                                    println!("[ERR] file with same name as video already exists");
                                    println!("[INFO] exiting");
                                    std::process::exit(69);
                                } else {
                                    if vid_data.status().is_success() {
                                        let mut file = File::create(&video_path).unwrap();

                                        match copy(&mut vid_data, &mut file) {
                                            Ok(_) => {
                                                println!("[INFO] download finished successfully")
                                            }
                                            Err(err) => println!(
                                                "[ERR] error while writing the downloaded file: {}",
                                                err
                                            ),
                                        }
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        println!("[ERR] unable to find video from this link");
                    }
                }
            }
            Err(err) => {
                println!("failed due to error: {}", err)
            }
        }
    }
}

fn req_headers() -> HeaderMap {
    let mut req_headers = HeaderMap::new();
    req_headers.insert(
        USER_AGENT,
        "entire cloudflare team is useless".parse().unwrap(),
    );
    req_headers.insert(ACCEPT, "*/*".parse().unwrap());
    req_headers
}
