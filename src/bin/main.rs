use serde::{Serialize, Deserialize};
// use smallvec::SmallVec;
use async_std::task;

// mod de;
// mod metadata;
// mod session;
// mod bitfield;
// mod utils;
// mod actors;
// mod pieces;
// mod supervisors;
// mod errors;
// mod extensions;
// mod bencode;
// mod udp_ext;

// use async_std::task;
use std::io::{self, Read};

use rustorrent::session::Session;
use rustorrent::de;

//fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
fn main() {
    let stdin = io::stdin();
    let mut buffer = Vec::new();
    let mut handle = stdin.lock();

    handle.read_to_end(&mut buffer).unwrap();

    //let (meta, info) = de::from_bytes_with_hash::<MetaTorrent>(&buffer).unwrap();
    let torrent = de::read_meta(&buffer).unwrap();

    println!("TORRENT={:#?}", torrent);

    let mut session = Session::new();

    session.add_torrent(torrent);

    let mut buffer = String::new();
    let stdin = io::stdin();
    let mut handle = stdin.lock();

    handle.read_to_string(&mut buffer).unwrap();
//     task::block_on(async move {
//         let mut res = surf::get("http://localhost:6969/announce")
// //        let mut res = surf::get(&meta.announce)
//             .set_query(&query)?
//             .recv_string()
//             .await?;

//         println!("{:#?}", res);

//         Ok(())
//     })
}