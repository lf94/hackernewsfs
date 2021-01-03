use {
  async_trait::async_trait,
  libc::{getgid, getuid, S_IFDIR},
  rs9p::{
    error::Error,
    srv::{srv_async, Fid, Filesystem},
    Fcall,
    Data,
    DirEntry,
    DirEntryData,
    Qid,
    QidType,
    Time,
    Result,
    Stat,
    GetattrMask
  },
  tokio::{
    sync::RwLock
  },
  hn_api::{
    types::Item as HNItem,
    types::Item::*,
  },
};

mod api;
mod utils;
mod walks;
mod entries;

use crate::entries::Entry;
use crate::entries::Entry::*;
use crate::walks::walk1_fragment;
use crate::api::get_items;
use crate::utils::{
  DirEntryConfig,
  FromMode,
};

#[derive(Clone)]
pub struct Crumb {
  entry: Entry,
  qpath: u64,
  mode: u32,
  data: Option<HNItem>,
}

impl Default for Crumb {
  fn default() -> Self {
    Crumb {
      entry: EUnknown,
      qpath: 0,
      mode: 0,
      data: None,
    }
  }
}

impl From<Crumb> for Qid {
  fn from(item: Crumb) -> Self {
    Qid {
      path: item.qpath,
      version: 0,
      typ: QidType::from_mode(item.mode),
    }
  }
}

#[derive(Default)]
struct HackernewsfsFid {
  crumb: RwLock<Crumb>,
}

#[derive(Clone)]
struct Hackernewsfs {
}

#[async_trait]
impl Filesystem for Hackernewsfs {
    type Fid = HackernewsfsFid;

    async fn rattach(
      &self,
      fid: &Fid<Self::Fid>,
      _afid: Option<&Fid<Self::Fid>>,
      _uname: &str,
      _aname: &str,
      _n_uname: u32,
    ) -> Result<Fcall> {
      let root = Crumb {
        entry: ERoot,
        qpath: ERoot as u64,
        mode: S_IFDIR | 0o555,
        data: None,
      };

      let qid = Qid::from(root.clone());
      
      *fid.aux.crumb.write().await = root;
      Ok(Fcall::Rattach { qid })
    }

    async fn rwalk(
      &self,
      fid: &Fid<Self::Fid>,
      newfid: &Fid<Self::Fid>,
      wnames: &[String],
    ) -> Result<Fcall> {
      let mut wqids = vec![];

      let crumb = fid.aux.crumb.read().await.clone();

      if wnames.len() == 0 {
        *newfid.aux.crumb.write().await = crumb;
        return Ok(Fcall::Rwalk { wqids });
      }

      match walk1_fragment(&wnames[0], crumb) {
        Some(result) => {
          wqids.push(Qid::from(result.clone()));
          *newfid.aux.crumb.write().await = result;
        },
        None => return Err(
          Error::Io(
            std::io::Error::new(std::io::ErrorKind::Other, "walk fail")
          )
        )
      }

      Ok(Fcall::Rwalk { wqids })
    }

    async fn rgetattr(&self, fid: &Fid<Self::Fid>, req_mask: GetattrMask) -> Result<Fcall> {
      let crumb = fid.aux.crumb.read().await.clone();

      let uid = unsafe { getuid() };
      let gid = unsafe { getgid() };

      let time = match crumb.clone().data {
        Some(Story(d)) => Time {
          sec: d.time,
          nsec: 0,
        },
        Some(Job(d)) => Time {
          sec: d.time,
          nsec: 0,
        },
        Some(Comment(d)) => Time {
          sec: d.time,
          nsec: 0,
        },
        _ => Time {
          sec: 0,
          nsec: 0,
        },
      };

      let stat = Stat {
        mode: crumb.mode,
        uid,
        gid,
        nlink: 0,
        rdev: 0,
        size: 0,
        blksize: 0,
        blocks: 0,
        atime: time,
        mtime: time,
        ctime: time,
      };
      

      Ok(Fcall::Rgetattr {
        valid: req_mask,
        qid: Qid::from(crumb),
        stat,
      })
    }

    async fn rlopen(&self, fid: &Fid<Self::Fid>, _flags: u32) -> Result<Fcall> {
      let crumb = fid.aux.crumb.read().await.clone();

      Ok(Fcall::Rlopen {
        qid: Qid::from(crumb),
        iounit: 0,
      })
    }

    async fn rread(&self, fid: &Fid<Self::Fid>, offset: u64, count: u32) -> Result<Fcall> {
      let crumb = fid.aux.crumb.read().await.clone();

      let item = crumb.data;

      // We're not tracking how much we've output, so if we've read at all,
      // we're done.
      if offset > 0 {
        return Ok(Fcall::Rread { data: Data(vec![]) });
      }
      
      let mut data: Vec<u8> = match item {
        Some(Story(d)) => match crumb.entry {
          EId => d.id.to_string().into_bytes(),
          ETitle => d.title.into_bytes(),
          EScore => d.score.to_string().into_bytes(),
          EText => match d.text {
            Some(t) => t.into_bytes(),
            _ => vec![],
          },
          ETime => d.time.to_string().into_bytes(),
          EUser => d.by.into_bytes(),
          EUrl => match d.url {
            Some(u) => u.into_bytes(),
            _ => vec![],
          },
          _ => vec![],
        },
        Some(Comment(d)) => match crumb.entry {
          EId => d.id.to_string().into_bytes(),
          EText => d.text.into_bytes(),
          ETime => d.time.to_string().into_bytes(),
          EUser => d.by.into_bytes(),
          _ => vec![],
        },
        Some(Job(d)) => match crumb.entry {
          EId => d.id.to_string().into_bytes(),
          ETitle => d.title.into_bytes(),
          EScore => d.score.to_string().into_bytes(),
          EText => match d.text {
            Some(t) => t.into_bytes(),
            _ => vec![],
          },
          ETime => d.time.to_string().into_bytes(),
          _ => vec![],
        },
        _ => vec![],
      };

      // Must fit within "count".
      data.truncate(count as usize);
     
      Ok(Fcall::Rread { data: Data(data) })
    }

    // The directory entries must fit within "count" - 100 fit so I'm going with
    // that limit. Better would be to use mem::size_of to calculate it.
    async fn rreaddir(&self, fid: &Fid<Self::Fid>, offset: u64, _count: u32) -> Result<Fcall> {
      let mut dirs = DirEntryData::new();
      let crumb = fid.aux.crumb.read().await.clone();

      if offset == 0 {
        dirs = match crumb.entry {
          ERoot => DirEntryData::with(vec![
            // DirEntry.offset is SUPER important to set. Otherwise readdir will
            // continue forever not knowing where it is.
            DirEntry::from(ETop).offset(0).typ(QidType::DIR),
            DirEntry::from(ENew).offset(1).typ(QidType::DIR),
            DirEntry::from(EBest).offset(2).typ(QidType::DIR),
            DirEntry::from(EAsk).offset(3).typ(QidType::DIR),
            DirEntry::from(EShow).offset(4).typ(QidType::DIR),
            DirEntry::from(EJob).offset(5).typ(QidType::DIR),
          ]),
          ETop | ENew | EBest | EAsk | EShow | EJob => {
            let mut ids = get_items(crumb.entry);
            DirEntryData::with(ids.drain(0..100).enumerate().map(|(index, id)|
              DirEntry {
                offset: index as u64,
                qid: Qid {
                  typ: QidType::from_mode(crumb.mode),
                  version: 0,
                  path: id,
                },
                typ: 0,
                name: format!("{}.{}", index + 1, id),
            }).collect())
          },
          EReplies => {
            let ids = match crumb.data.clone() {
              Some(Story(d)) => d.kids,
              Some(Comment(d)) => d.kids,
              _ => None
            }.or(Some(vec![])).unwrap();
            
            DirEntryData::with(ids.iter().enumerate().map(|(index, id)|
              DirEntry {
                offset: index as u64,
                qid: Qid {
                  typ: QidType::from_mode(crumb.mode),
                  version: 0,
                  path: (*id) as u64,
                },
                typ: 0,
                name: format!("{}.{}", index + 1, *id),
            }).collect())
          },
          EArticle => {
            let o = 0;
            let mut entries = vec![
              DirEntry::from(ETitle).offset(o),
              DirEntry::from(EScore).offset(o + 1),
              DirEntry::from(EUser).offset(o + 1),
              DirEntry::from(ETime).offset(o + 1),
            ];

            match crumb.data {
              Some(Story(d)) => {
                if d.url.is_some() {
                  entries.push(DirEntry::from(EUrl).offset(o + 1));
                }
                if d.text.is_some() {
                  entries.push(DirEntry::from(EText).offset(o + 1));
                }
                if d.kids.is_some() {
                  entries.push(
                    DirEntry::from(EReplies).offset(o + 1).typ(QidType::DIR)
                  );
                }
              },
              _ => ()
            };
            
            DirEntryData::with(entries)
          },
          EReply => DirEntryData::with(vec![
            DirEntry::from(EUser).offset(0),
            DirEntry::from(ETitle).offset(1),
            DirEntry::from(EText).offset(2),
            DirEntry::from(ETime).offset(3),
            DirEntry::from(EReplies).offset(4).typ(QidType::DIR),
          ]),
          _ => dirs
        }
      }

      Ok(Fcall::Rreaddir { data: dirs })
    }

    async fn rclunk(&self, _: &Fid<Self::Fid>) -> Result<Fcall> {
      Ok(Fcall::Rclunk)
    }
}

async fn hackernewsfs_main(args: Vec<String>) -> rs9p::Result<i32> {
    if args.len() < 2 {
        eprintln!("Usage: {} proto!address!port", args[0]);
        eprintln!("  where: proto = tcp | unix");
        return Ok(-1);
    }

    let addr = &args[1];

    println!("[*] Ready to accept clients: {}", addr);
    srv_async(
        Hackernewsfs { },
        addr,
    )
    .await
    .and(Ok(0))
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args = std::env::args().collect();
    let exit_code = hackernewsfs_main(args).await.unwrap_or_else(|e| {
        eprintln!("Error: {:?}", e);
        -1
    });

    std::process::exit(exit_code);
}
