use {
  rs9p::{
    DirEntry,
    Qid,
    QidType,
  },
  libc::S_IFDIR,
  crate::entries::Entry,
};

pub trait FromMode {
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

pub trait DirEntryConfig {
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
