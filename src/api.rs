use {
  hn_api::{
    HnClient,
    types::Item as HNItem,
    types::Item::*,
  },
  crate::entries::Entry,
  crate::entries::Entry::*,
};

pub fn get_items(entry: Entry) -> Vec<u64> {
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

pub fn get_item(id: u64) -> Option<HNItem> {
  let client = HnClient::init().unwrap();
  client.get_item(id as u32).unwrap()
}

pub fn get_replies(item: Option<HNItem>) -> Vec<u32> {
  match item {
    Some(Story(d)) => d.kids,
    Some(Comment(d)) => d.kids,
    _ => None,
  }.or(Some(vec![])).unwrap()
}

