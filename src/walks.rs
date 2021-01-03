use {
  crate::Crumb,
  crate::entries::Entry::*,
  libc::{S_IFDIR, S_IFREG},
  crate::api::{get_item, get_items, get_replies},
};

pub fn walk1_category(part: &str, crumb: Crumb) -> Option<Crumb> {
  let realpart = part.split('.').collect::<Vec<&str>>()[1];
  let returned = get_items(crumb.entry);
  for id in returned.iter() {
    if realpart == id.to_string() {
      return Some(Crumb {
        entry: EArticle,
        qpath: (*id) as u64,
        mode: S_IFDIR | 0o555,
        data: None,
      });
    }
  }
  None
}

pub fn walk1_replies(part: &str, crumb: Crumb) -> Option<Crumb> {
  let tid = part.split('.').collect::<Vec<&str>>()[1];
  let replies = get_replies(crumb.data);
  
  for id in replies.iter() {
    if tid == id.to_string() {
      return Some(Crumb {
        entry: EReply,
        qpath: (*id) as u64,
        mode: S_IFDIR | 0o555,
        data: None,
      });
    }
  }
  None
}

pub fn walk1_fragment(
  fragment: &str,
  crumb: Crumb
) -> Option<Crumb> {
  match crumb.entry {
    ERoot => match fragment {
      "top" => Some(Crumb { entry: ETop, qpath: ETop as u64, mode: S_IFDIR | 0o555, data: None }),
      "new" => Some(Crumb { entry: ENew, qpath: ENew as u64, mode: S_IFDIR | 0o555, data: None }),
      "best" => Some(Crumb { entry: EBest, qpath: EBest as u64, mode: S_IFDIR | 0o555, data: None }),
      "ask" => Some(Crumb { entry: EAsk, qpath: EAsk as u64, mode: S_IFDIR | 0o555, data: None }),
      "show" => Some(Crumb { entry: EShow, qpath: EShow as u64, mode: S_IFDIR | 0o555, data: None }),
      "job" => Some(Crumb { entry: EJob, qpath: EJob as u64, mode: S_IFDIR | 0o555, data: None }),
      _ => None
    },
    ETop | ENew | EBest | EAsk | EShow | EJob =>
      walk1_category(fragment, crumb),
    EReplies =>
      walk1_replies(fragment, crumb),
    EArticle => {
      let data = get_item(crumb.qpath);
      
      match fragment {
        "id" => Some(Crumb {
          entry: EId,
          qpath: crumb.qpath << 8 | (EId as u64),
          mode: S_IFREG | 0o444,
          data,
        }),
        "user" => Some(Crumb {
          entry: EUser,
          qpath: crumb.qpath << 8 | (EUser as u64),
          mode: S_IFREG | 0o444,
          data,
        }),
        "title" => Some(Crumb {
          entry: ETitle,
          qpath: crumb.qpath << 8 | (ETitle as u64),
          mode: S_IFREG | 0o444,
          data,
        }),
        "replies" => Some(Crumb {
          entry: EReplies,
          qpath: crumb.qpath << 8 | (EReplies as u64),
          mode: S_IFDIR | 0o555,
          data,
        }),
        "text" => Some(Crumb {
          entry: EText,
          qpath: crumb.qpath << 8 | (EText as u64),
          mode: S_IFREG | 0o444,
          data,
        }),
        "created_at" => Some(Crumb {
          entry: ETime,
          qpath: crumb.qpath << 8 | (ETime as u64),
          mode: S_IFREG | 0o444,
          data,
        }),
        "score" => Some(Crumb {
          entry: EScore,
          qpath: crumb.qpath << 8 | (EScore as u64),
          mode: S_IFREG | 0o444,
          data,
        }),
        "url" => Some(Crumb {
          entry: EUrl,
          qpath: crumb.qpath << 8 | (EUrl as u64),
          mode: S_IFREG | 0o444,
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
          mode: S_IFREG | 0o444,
          data,
        }),
        "user" => Some(Crumb {
          entry: EUser,
          qpath: crumb.qpath << 8 | (EUser as u64),
          mode: S_IFREG | 0o444,
          data,
        }),
        "text" => Some(Crumb {
          entry: EText,
          qpath: crumb.qpath << 8 | (EText as u64),
          mode: S_IFREG | 0o444,
          data,
        }),
        "created_at" => Some(Crumb {
          entry: ETime,
          qpath: crumb.qpath << 8 | (ETime as u64),
          mode: S_IFREG | 0o444,
          data,
        }),
        "replies" => Some(Crumb {
          entry: EReplies,
          qpath: crumb.qpath << 8 | (EReplies as u64),
          mode: S_IFDIR | 0o555,
          data,
        }),
        _ => None
      }
    },
    EId | EUser | EScore | ETitle | EUrl | EText | ETime => None,
    EUnknown => None,
  }
}
