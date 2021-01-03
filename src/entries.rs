#[derive(Clone, Copy)]
pub enum Entry {
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
  EId,
  EUser,
  EReplies,
  EScore,
  ETitle,
  EUrl,
  EText,
  ETime,
}

use Entry::*;

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
