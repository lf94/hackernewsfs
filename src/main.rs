use {
  async_trait::async_trait,
  libc::{
    getgid, getuid, S_IRUSR, S_IXUSR, S_IRGRP, S_IXGRP, S_IROTH, S_IXOTH,
    S_IFREG, S_IFDIR
  },
  rs9p::{
    error::{
      Error,
    },
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
  std::path::PathBuf,
  tokio::{
    fs,
    sync::RwLock
  },
  hn_api::{
    HnClient,
    types::Item as HNItem,
    types::Item::*,
  },
};

// Saves some typing.
const RDDIR: u32 = S_IFDIR | S_IROTH | S_IXOTH | S_IRGRP | S_IXGRP | S_IRUSR | S_IXUSR;
const RDFILE: u32 = S_IFREG | S_IROTH |  S_IRGRP |  S_IRUSR;

// v9fs (or the shell (bash)) takes care of ".." and "." traversal.

#[derive(Clone, Copy, Debug)]
enum Entry {
  // Directories
  EUnknown,
  ERoot,
  ETop,
  ENew,
  EBest,
  EAsk,
  EShow,
  EJob,
  EArticle,
  EReply,

  // Files
  // Story / Comment / Job all share these
  EId,
  EUser,
  EReplies,
  EScore,
  ETitle,
  EUrl,
  EText,
  ETime,

  // No support for polls because I've never seen them...
}

use Entry::*;

// For translating between directory names and entries
impl From<String> for Entry {
  fn from(item: String) -> Self {
    match item.as_str() {
      "/" => ERoot,
      "top" => ETop,
      "new" => ENew,
      "best" => EBest,
      "ask" => EAsk,
      "show" => EShow,
      "job" => EJob,
      "id" => EId,
      "title" => ETitle,
      "replies" => EReplies,
      "score" => EScore,
      "text" => EText,
      "user" => EUser,
      "created_at" => ETime,
      "url" => EUrl,
      _ => EUnknown,
    }
  }
}

impl From<Entry> for String {
  fn from(item: Entry) -> Self {
    match item {
      ERoot => "/",
      ETop => "top",
      ENew => "new",
      EBest => "best",
      EAsk => "ask",
      EShow => "show",
      EJob => "job",
      EId => "id",
      ETitle => "title",
      EUser => "user",
      EReplies => "replies",
      EScore => "score",
      EText => "text",
      ETime => "created_at",
      EUrl => "url",
      _ => "",
    }.to_string()
  }
}

#[derive(Clone, Debug)]
struct Crumb {
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

// Map modes to Qidtype
trait FromMode {
  fn from_mode(i: u32) -> Self;
}

impl FromMode for QidType {
  fn from_mode(item: u32) -> Self {
    if item & S_IFDIR == S_IFDIR {
      QidType::DIR
    } else {
      QidType::FILE
    }
  }
}

// Make it easier to create DirEntry from Crumb
impl From<Entry> for DirEntry {
  fn from(item: Entry) -> Self {
    DirEntry {
      offset: 0,
      qid: Qid {
        version: 0,
        typ: QidType::FILE,
        path: item as u64,
      },
      typ: 0,
      name: String::from(item)
    }
  }
}

trait DirEntryConfig {
  fn offset(&mut self, o: u64) -> DirEntry;
  fn typ(&mut self, qt: QidType) -> DirEntry;
}

impl DirEntryConfig for DirEntry {
  fn offset(&mut self, o: u64) -> DirEntry {
    self.offset = o;
    return self.clone();
  }
  fn typ(&mut self, qt: QidType) -> DirEntry {
    self.qid.typ = qt;
    return self.clone();
  }
}

// Requests to HN / Firebase
fn get_items(entry: Entry) -> Vec<u64> {
  let client = HnClient::init().unwrap();
  let ids = match entry {
    ETop => client.get_top_stories().unwrap(),
    EBest => client.get_best_stories().unwrap(),
    ENew => client.get_new_stories().unwrap(),
    EJob => client.get_job_stories().unwrap(),
    EAsk => client.get_ask_stories().unwrap(),
    EShow => client.get_show_stories().unwrap(),
    _ => vec![]
  };
  ids.iter().map(|i| *i as u64).collect()
}

fn get_item(id: u64) -> Option<HNItem> {
  let client = HnClient::init().unwrap();
  client.get_item(id as u32).unwrap()
}

// walk1 is a convention in plan 9 that means "walk 1 element".
// It's easier to reason about walking 1 element than many elements.
// 
// v9fs takes care of calling walk N times.
fn walk1_category(part: &str, crumb: Crumb) -> Option<Crumb> {
  let realpart = part.split('.').collect::<Vec<&str>>()[1];
  let returned = get_items(crumb.entry);
  for id in returned.iter() {
    if realpart == id.to_string() {
      return Some(Crumb {
          entry: EArticle,
          qpath: (*id) as u64,
          mode: RDDIR,
          data: None,
      });
    }
  }
  None
}

fn walk1_replies(part: &str, crumb: Crumb) -> Option<Crumb> {
  let realpart = part.split('.').collect::<Vec<&str>>()[1];
  let returned = match crumb.data {
    Some(Story(d)) => d.kids,
    Some(Comment(d)) => d.kids,
    _ => None
  }.or(Some(vec![])).unwrap();
  
  for id in returned.iter() {
    if realpart == id.to_string() {
      return Some(Crumb {
          entry: EReply,
          qpath: (*id) as u64,
          mode: RDDIR,
          data: None,
      });
    }
  }
  None
}

fn walk1_fragment(
  fragment: &str,
  crumb: Crumb
) -> Option<Crumb> {
  match crumb.entry {
    ERoot => match fragment {
      "top" => Some(Crumb { entry: ETop, qpath: ETop as u64, mode: RDDIR, data: None }),
      "new" => Some(Crumb { entry: ENew, qpath: ENew as u64, mode: RDDIR, data: None }),
      "best" => Some(Crumb { entry: EBest, qpath: EBest as u64, mode: RDDIR, data: None }),
      "ask" => Some(Crumb { entry: EAsk, qpath: EAsk as u64, mode: RDDIR, data: None }),
      "show" => Some(Crumb { entry: EShow, qpath: EShow as u64, mode: RDDIR, data: None }),
      "job" => Some(Crumb { entry: EJob, qpath: EJob as u64, mode: RDDIR, data: None }),
      _ => None
    },
    ETop | ENew | EBest | EAsk | EShow | EJob =>
      walk1_category(fragment, crumb),
    EReplies =>
      walk1_replies(fragment, crumb),
    EArticle => {
      let data = get_item(crumb.qpath);
      
      match fragment {
        // Need a better way to calculate unique qid...
        // It's possible for "data loss" here because of shift
        // Could also remove a lot of repetition.
        "id" => Some(Crumb {
          entry: EId,
          qpath: crumb.qpath << 8 | (EId as u64),
          mode: RDFILE,
          data,
        }),
        "user" => Some(Crumb {
          entry: EUser,
          qpath: crumb.qpath << 8 | (EUser as u64),
          mode: RDFILE,
          data,
        }),
        "title" => Some(Crumb {
          entry: ETitle,
          qpath: crumb.qpath << 8 | (ETitle as u64),
          mode: RDFILE,
          data,
        }),
        "replies" => Some(Crumb {
          entry: EReplies,
          qpath: crumb.qpath << 8 | (EReplies as u64),
          mode: RDDIR,
          data,
        }),
        "text" => Some(Crumb {
          entry: EText,
          qpath: crumb.qpath << 8 | (EText as u64),
          mode: RDFILE,
          data,
        }),
        "created_at" => Some(Crumb {
          entry: ETime,
          qpath: crumb.qpath << 8 | (ETime as u64),
          mode: RDFILE,
          data,
        }),
        "score" => Some(Crumb {
          entry: EScore,
          qpath: crumb.qpath << 8 | (EScore as u64),
          mode: RDFILE,
          data,
        }),
        "url" => Some(Crumb {
          entry: EUrl,
          qpath: crumb.qpath << 8 | (EUrl as u64),
          mode: RDFILE,
          data,
        }),
        _ => None
      }
    },
    EReply => {
      let data = get_item(crumb.qpath);
      
      match fragment {
        "id" => Some(Crumb {
          entry: EId,
          qpath: crumb.qpath << 8 | (EId as u64),
          mode: RDFILE,
          data,
        }),
        "user" => Some(Crumb {
          entry: EUser,
          qpath: crumb.qpath << 8 | (EUser as u64),
          mode: RDFILE,
          data,
        }),
        "text" => Some(Crumb {
          entry: EText,
          qpath: crumb.qpath << 8 | (EText as u64),
          mode: RDFILE,
          data,
        }),
        "created_at" => Some(Crumb {
          entry: ETime,
          qpath: crumb.qpath << 8 | (ETime as u64),
          mode: RDFILE,
          data,
        }),
        "replies" => Some(Crumb {
          entry: EReplies,
          qpath: crumb.qpath << 8 | (EReplies as u64),
          mode: RDDIR,
          data,
        }),
        _ => None
      }
    },
    EId | EUser | EScore | ETitle | EUrl | EText | ETime => None,
    EUnknown => None,
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
      println!("rattach");

      let root = Crumb {
        entry: ERoot,
        qpath: ERoot as u64,
        mode: RDDIR,
        data: None,
      };
      let qid = Qid {
        typ: QidType::from_mode(root.mode),
        version: 0,
        path: root.qpath,
      };
      
      *fid.aux.crumb.write().await = root;
      Ok(Fcall::Rattach { qid })
    }

    //
    // v9fs will "split up" the walk commands so that we don't have to worry
    // about handling many path parts, which simplifies the code.
    // 
    // The code below is thus written to only consider wnames.len() == 1.
    // 
    // 9p clients expect the server to do "best effort" for walking, i.e. return
    // a list of Qid which match up until the point of failure. We don't need to
    // worry about this because of the earlier situation mentioned.
    //
    // The best reference for how walk should behave is in the Plan 9
    // 'man 5 walk' section.
    // 
    async fn rwalk(
      &self,
      fid: &Fid<Self::Fid>,
      newfid: &Fid<Self::Fid>,
      wnames: &[String],
    ) -> Result<Fcall> {
      let mut wqids = vec![];

      // Get the associated data with this fid.
      let crumb = fid.aux.crumb.read().await.clone();

      // If there are no wnames we are just cloning the current fid, and no
      // walking is happening. The fid could represent a file or directory.
      if wnames.len() == 0 {
        *newfid.aux.crumb.write().await = crumb;
        return Ok(Fcall::Rwalk { wqids });
      }

      // Do one walk, since wnames.len() could only be 1 at this point.
      // IMPROVE: Change return type of walk1_fragment to Result<T>.
      // Right now it doesn't distinguish between different failures.
      let wr = walk1_fragment(&wnames[0], crumb);
      match wr.clone() {
        Some(result) => wqids.push(Qid {
          typ: QidType::from_mode(result.mode),
          path: result.qpath,
          version: 0,
        }),
        // So, SO ugly. Why 3 levels of error type nesting??
        None => return Err(
          Error::Io(
            std::io::Error::new(std::io::ErrorKind::Other, "walk fail")
          )
        )
      }

      // If we had a successful full walk, set the newfid.
      if wnames.len() == wqids.len() {
        if let Some(t) = wr {
          *newfid.aux.crumb.write().await = t;
        }
      }
      

      Ok(Fcall::Rwalk { wqids })
    }

    // The attributes of all directories and files are pretty much read-only.
    // Execution bits are needed on directories because they have a different
    // meaning: that content can be read but not listed.
    async fn rgetattr(&self, fid: &Fid<Self::Fid>, req_mask: GetattrMask) -> Result<Fcall> {
      let crumb = fid.aux.crumb.read().await.clone();

      let uid = unsafe { getuid() };
      let gid = unsafe { getgid() };

      // https://pubs.opengroup.org/onlinepubs/9699919799/basedefs/sys_stat.h.html
      // https://www.gnu.org/software/libc/manual/html_node/Permission-Bits.html
      // libc exports these! See crates.io.
      // Read the above to understand what mode is.
      
      let stat = Stat {
        mode: crumb.mode,

        // Use the uid and gid of the user who started the program
        // These are only used in 9p2000.u ...
        // v9fs ignores these otherwise.
        uid,
        gid,

        // Since we are creating synthetic files, it's ok to leave them 0.
        // Of course you could fill these with whatever is appropriate if you
        // want to put the extra effort in!
        nlink: 0,
        rdev: 0,
        size: 0,
        blksize: 0,
        blocks: 0,

        // TODO: Use the current time.
        atime: Time {
          sec: 0,
          nsec: 0,
        },

        // Should never change if the directory cannot be manipulated.
        // Should only be set once (initial access/creation).
        mtime: Time {
          sec: 0,
          nsec: 0,
        },

        // Same as mtime
        ctime: Time {
          sec: 0,
          nsec: 0,
        },
      };
      
      println!("rgetattr");
      println!("{:?}", stat);

      Ok(Fcall::Rgetattr {
        valid: req_mask, // Any attrs requested are valid.
        qid: Qid {
          typ: QidType::from_mode(crumb.mode),
          version: 0,
          path: crumb.qpath,
        },
        stat,
      })
    }

    //
    // Prepare file for operations.
    // In our case everything is readable, and we just ignore it anyway.
    //
    // Called before both readdir and read
    // 
    async fn rlopen(&self, fid: &Fid<Self::Fid>, _flags: u32) -> Result<Fcall> {
      println!("rlopen");

      let crumb = fid.aux.crumb.read().await.clone();

      Ok(Fcall::Rlopen {
        qid: Qid {
          typ: QidType::from_mode(crumb.mode),
          version: 0,
          path: crumb.qpath
        },
        iounit: 0 /* No limit on bytes read or written */,
      })
    }

    // Absolutely requires you to return a buffer the size of "count"!
    async fn rread(&self, fid: &Fid<Self::Fid>, offset: u64, count: u32) -> Result<Fcall> {
      println!("rread {:?} {:?}", offset, count);
      let crumb = fid.aux.crumb.read().await.clone();

      let item = crumb.data;

      // If we've already done a read, we're done.
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

      // None of the data has newlines in it...
      data.push(0x0A);
      
      Ok(Fcall::Rread { data: Data(data) })
    }

    // Is *always* called *at least* twice. Once for initial listing, once to
    // signal no more entries.
    async fn rreaddir(&self, fid: &Fid<Self::Fid>, offset: u64, count: u32) -> Result<Fcall> {
      println!("rreaddir");
      println!("{:?} {:?}", offset, count);
      
      let mut dirs = DirEntryData::new();
      let crumb = fid.aux.crumb.read().await.clone();

      println!("{:?}", crumb);

      // If it's the initial listing, fill up our entry listing.
      // Otherwise, our entry listing will be empty, meaning no more.
      if offset == 0 {
        dirs = match crumb.entry {
          ERoot => DirEntryData::with(vec![
            // DirEntry.offset is SUPER important to set. Otherwise readdir will
            // continue forever not knowing where it is.
            DirEntry::from(ETop).offset(2).typ(QidType::DIR),
            DirEntry::from(ENew).offset(3).typ(QidType::DIR),
            DirEntry::from(EBest).offset(4).typ(QidType::DIR),
            DirEntry::from(EAsk).offset(5).typ(QidType::DIR),
            DirEntry::from(EShow).offset(6).typ(QidType::DIR),
            DirEntry::from(EJob).offset(7).typ(QidType::DIR),
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
          EArticle => DirEntryData::with(vec![
            DirEntry::from(ETitle).offset(0),
            DirEntry::from(EScore).offset(1),
            DirEntry::from(EUser).offset(2),
            DirEntry::from(EUrl).offset(3),
            DirEntry::from(EText).offset(4),
            DirEntry::from(ETime).offset(5),
            DirEntry::from(EReplies).offset(6).typ(QidType::DIR),
          ]),
          EReply => DirEntryData::with(vec![
            DirEntry::from(EUser).offset(1),
            DirEntry::from(ETitle).offset(2),
            DirEntry::from(EText).offset(3),
            DirEntry::from(ETime).offset(4),
            DirEntry::from(EReplies).offset(5).typ(QidType::DIR),
          ]),
          _ => dirs
        }
      }

      Ok(Fcall::Rreaddir { data: dirs })
    }

    // Since we are not tracking any resources (other than the fid data)
    // there is nothing to free when we "clunk" a file.
    async fn rclunk(&self, _: &Fid<Self::Fid>) -> Result<Fcall> {
      println!("rclunk");
      Ok(Fcall::Rclunk)
    }
}

async fn hackernewsfs_main(args: Vec<String>) -> rs9p::Result<i32> {
    if args.len() < 2 {
        eprintln!("Usage: {} proto!address!port", args[0]);
        eprintln!("  where: proto = tcp | unix");
        return Ok(-1);
    }

    let (addr) = (&args[1]);

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
