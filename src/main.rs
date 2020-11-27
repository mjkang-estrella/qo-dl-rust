use std::io::{self, copy, Read, Write};
use std::thread;
use std::process;
use std::time::Duration;
use std::fs::File;
use std::string::String;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use std::fs::{create_dir, remove_file};

//External Libraries
use reqwest::header;
use reqwest::header::HeaderMap;
use serde_json::{Value};
use md5;
use metaflac::Tag;
use metaflac::block::PictureType::CoverFront;
use indicatif::{ProgressBar, ProgressStyle};

const APP_ID: &str = "793410592";
const ID: &str = "kang.damiano@gmail.com"; // Email
const PW: &str = "e207c26cc49e7c25dd7442fc1dac2161"; // MD5 Hashed

struct DownloadProgress<R> {
    inner: R,
    progress_bar: ProgressBar,
}

//Change Read as giving data to Progress bar
impl<R> Read for DownloadProgress<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf).map(|n| {
            self.progress_bar.inc(n as u64);
            n
        })
    }
}

//Download file from the URL
fn downloader(url: String, dir: PathBuf, header: HeaderMap){
    let client = reqwest::Client::new();
    let total_size = {
        let resp = client.head(&url).send().unwrap();
        resp.headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|ct_len| ct_len.to_str().ok())
            .and_then(|ct_len| ct_len.parse().ok())
            .unwrap_or(0)
    };

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
                 .template("{spinner:.green} [{elapsed_precise}] [{bar:40.yellow/blue}] {bytes}/{total_bytes} ({eta})")
                 .progress_chars("#>-"));
    let mut file_out = File::create(dir).expect("Failed to create file");
    let file_resp = client.get(&url).headers(header).send().expect("Failed to download file...");

    let mut source = DownloadProgress {
        progress_bar: pb,
        inner: file_resp,
    };

    copy(&mut source, &mut file_out).expect("Failed to download contents...");
}

//Delete useless letters from metadata
fn cleaner(some_letters: String) -> String{
    let mut result = some_letters.clone();
    result.retain(|c| c!= '"' && c!= '/' && c!= '\\');
    return result;
}

//inserting metadata into file
fn insert_metadata(file_path: PathBuf, album_cover_path: PathBuf, meta: HashMap<String, String>){
    let mut tag = Tag::read_from_path(file_path).expect("Error on reading tag");
    for (name, value) in meta{
        tag.set_vorbis(name.to_uppercase(), vec!(value));
    }
    let image = std::fs::read(album_cover_path).expect("Cannot open album cover...");
    tag.add_picture("image/jpg", CoverFront, image);
    tag.save().expect("Error on saving tag");
}

//download album
fn rip(album_id: String, header: HeaderMap){
    let format_id = "27";
    let client = reqwest::Client::new();
    let album_url = format!("https://play.qobuz.com/album/{}", album_id);
    let header0 = header.clone();
    let mut response = client.get("https://www.qobuz.com/api.json/0.2/album/get?")
        .query(&[("album_id", album_id)])
        .headers(header0)
        .send()
        .expect("Error on getting album id");
    let album_data: Value = response.json().expect("Error on JSON");

    //Streamable Check
    if album_data["code"] == 404 || album_data["streamable"].as_bool() == Some(false) {
        println!("Album does not appear to be streamable");
        thread::sleep(Duration::from_secs(2));
        return;
    }

    let album_cover_url0 = album_data["image"]["large"].to_string();
    let album_cover_url = format!("{}max.jpg", album_cover_url0[1..album_cover_url0.len()-8].to_string());
    
    let base_dir = PathBuf::from("./Albums");
    let _ =create_dir(&base_dir);

    //getting metadatas for make folders
    let tracks = album_data["tracks"]["items"].as_array().unwrap();
    let mut metadata = HashMap::new();
    metadata.insert(String::from("Album"), cleaner(album_data["title"].to_string()));
    metadata.insert(String::from("Albumartist"), cleaner(album_data["artist"]["name"].to_string()));
    metadata.insert(String::from("Genre"), cleaner(album_data["genre"]["name"].to_string()));
    metadata.insert(String::from("Organization"), cleaner(album_data["label"]["name"].to_string()));
    metadata.insert(String::from("Tracktotal"), tracks.len().to_string());
    
    let mut album_download_dir = base_dir.clone();
    let folder_title = format!("{} - {}", metadata["Albumartist"], metadata["Album"]);
    album_download_dir.push(Path::new(&folder_title));
    let _ = create_dir(&album_download_dir);

    //Downloading album cover for metadata
    let mut album_cover_dir = album_download_dir.clone();
    album_cover_dir.push("cover.jpg");
    let empty_header = HeaderMap::new();
    let album_cover_dir0 = album_cover_dir.clone();
    downloader(album_cover_url, album_cover_dir0, empty_header);

    //Downloading Tracks!!!!!
    println!("{}: ", folder_title);
    
    for (index, track) in tracks.iter().enumerate(){
        let mut download_headers = header.clone();
        download_headers.insert(header::HeaderName::from_static("range"), header::HeaderValue::from_static("bytes=0-"));
        download_headers.insert(header::HeaderName::from_static("user-agent"), header::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:67.0) Gecko/20100101 Firefox/67.0"));
        download_headers.insert(header::HeaderName::from_static("referer"), header::HeaderValue::from_str(&album_url).unwrap());
        let download_headers0 = download_headers.clone();

        let track_number = (index + 1).to_string();
        let current_time = SystemTime::now().duration_since(UNIX_EPOCH).expect("Error on getting time...").as_secs().to_string();
        let reqsigt = format!("trackgetFileUrlformat_id{}intentstreamtrack_id{}{}0e47db7842364064b7019225eb19f5d2", format_id, track["id"], current_time);
        let reqsighst = format!("{:x}", md5::compute(reqsigt));

        let mut tr_metadata = metadata.clone();
        tr_metadata.insert(String::from("Artist"), cleaner(track["performer"]["name"].to_string()));
        tr_metadata.insert(String::from("Composer"), cleaner(track["composer"]["name"].to_string()));
        tr_metadata.insert(String::from("Copyright"), cleaner(track["copyright"].to_string()));
        tr_metadata.insert(String::from("Title"), cleaner(track["title"].to_string()));
        tr_metadata.insert(String::from("Tracknumber"), track_number);
        tr_metadata.insert(String::from("Isrc"), cleaner(track["isrc"].to_string()));

        let mut responset = client.get("https://www.qobuz.com/api.json/0.2/track/getFileUrl?")
            .query(&[("request_ts", current_time),
                    ("request_sig", reqsighst),
                    ("track_id", track["id"].to_string()),
                    ("format_id", format_id.to_string()),
                    ("intent", String::from("stream"))])
            .headers(download_headers)
            .send()
            .expect("Error on getting file");
		let tr: Value = responset.json().expect("Error on JSON");
        let track_url0 = tr["url"].to_string();
        let track_url = track_url0[1..track_url0.len()-1].to_string();
        let file_name = format!("{} - {}.flac", tr_metadata["Tracknumber"], tr_metadata["Title"]);
        let mut track_dir = album_download_dir.clone();
        let track_format = format!("{} bits / {} kHz - {} channels", tr["bit_depth"].to_string(), tr["sampling_rate"].to_string(), track["maximum_channel_count"].to_string());
        track_dir.push(Path::new(&file_name));

        //Check Copyright
        if tr["restrictions"].to_string().find("TrackRestrictedByRightHolders") != Option::None || tr["sample"].as_bool() == Some(true){
            println!("Track {} is restricted by right holders. Can't download.", tr_metadata["Tracknumber"]);
            continue;
        }

        //Downloading...
        println!("Downloading track {} of {}: {} - {}", tr_metadata["Tracknumber"], tr_metadata["Tracktotal"], tr_metadata["Title"], track_format);
        let track_dir0 = track_dir.clone();
        downloader(track_url, track_dir0, download_headers0);
        
        //Putting metatdata to FLAC file
        let album_cover_dir1 = album_cover_dir.clone();
        insert_metadata(track_dir, album_cover_dir1, tr_metadata);
    }
    remove_file(album_cover_dir.as_path()).expect("Error on deleting album cover...");

    //Download Booklet...
    if album_data["goodies"][0]["file_format_id"].to_string() == "21"{
        println!("Booklet available, downloading...");
        let mut book_dir = album_download_dir.clone();
        book_dir.push("booklet.pdf");
        let book_url = album_data["goodies"][0]["original_url"].to_string();
        let empty_header0 = HeaderMap::new();
        downloader(book_url[1..book_url.len()-1].to_string(), book_dir, empty_header0);
    }
}

fn main() {
    
    let mut headers = header::HeaderMap::new();
    headers.insert(header::USER_AGENT, header::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:67.0) Gecko/20100101 Firefox/67.0"));

    // get a client
    let client = reqwest::Client::new();
    let session = client.get("https://www.qobuz.com/api.json/0.2/user/login?")
        .headers(headers);
    let mut res = session.query(&[("email", ID), ("password", PW), ("app_id", APP_ID)])
        .send()
        .expect("Error on Getting Client...");
    let rc = res.status();

    if rc.is_client_error(){
        println!("Bad credentials. Exiting...");
        thread::sleep(Duration::from_secs(2));
        process::exit(0);
    }
    else if rc.is_success(){
        let ssc0: Value = res.json().expect("Error on JSON");
        let ssc1 = &ssc0["user"]["credential"]["parameters"]["label"].to_string();
        let user_auth_token0 = &ssc0["user_auth_token"].to_string();
        let user_auth_token = user_auth_token0[1..user_auth_token0.len()-1].to_string();
        println!("Signed in successfully - {} account. \n", ssc1);
        
        //Sign in Process ended... Going to get album link
        
        loop {
            let mut album_id0 = String::new();
            print!("Input Qobuz Album ID: https://play.qobuz.com/album/");
            io::stdout().flush().unwrap();
            io::stdin().read_line(&mut album_id0)
                .expect("Failed to read line");

            if album_id0 == "quit\n"{
                return;
            }

            let album_id = album_id0[..album_id0.len()-1].to_string();

            let mut headers2 = header::HeaderMap::new();
            headers2.insert(header::USER_AGENT, header::HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:67.0) Gecko/20100101 Firefox/67.0"));
            headers2.insert(header::HeaderName::from_static("x-app-id"), header::HeaderValue::from_static(APP_ID));
            headers2.insert(header::HeaderName::from_static("x-user-auth-token"), header::HeaderValue::from_str(&user_auth_token).unwrap());
            rip(album_id, headers2);

            println!("Returning to URL Input Screen...");
            thread::sleep(Duration::from_secs(1));
            std::process::Command::new("clear").spawn().unwrap();
            thread::sleep(Duration::from_secs(1));
        }
        
    }
}