use futures::StreamExt;
use telegram_bot::*;
use mongodb::{Client, options::ClientOptions, bson::{doc, Bson, Array}, Collection, bson};
use mongodb::error::Error;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument};
use core::fmt;
use std::fmt::Formatter;
use rand::seq::SliceRandom;

const COMMAND_LIST: &str = "/list \n/help \n/random \n/clear \n/new <word> ";

#[tokio::main]
async fn main() -> Result<(), telegram_bot::Error> {
    eprintln!("TEST ERROR CHECK!!!");
    let collection = connect_to_db().await.unwrap();
    let token = "1218027891:AAE40Ml4He8_2gHqTOCtNOB3k5Dj2g1NgqQ";
    let api = Api::new(token);
    // Fetch new updates via long poll method
    let mut stream = api.stream();
    while let Some(update) = stream.next().await {
        // If the received update contains a new message...
        let update = update?;
        if let UpdateKind::Message(message) = update.kind {
            if let MessageKind::Text { ref data, ref entities } = message.kind {
                let chat = message.chat;
                println!("[DEBUG]------> <{}>: data: {} , entities: {:?}", &message.from.id, data, entities);
                if data.as_str().starts_with("/new ") {
                    let clear_word_string = &data.as_str()[4..].trim();//4 because need remove first char '/new'
                    println!("[DEBUG]------> clear_word_string: {}", clear_word_string);
                    let new_word_list = save_word(&message.from, clear_word_string, &collection).await.unwrap();
                    let new_words_arr = WordsUserFriendly::new(new_word_list.as_document().unwrap().get_array("words").unwrap());
                    api.send(chat.text(format!("I save you word:) \nYou new word list: {} ", new_words_arr))).await?;
                } else {
                    match data.as_str() {
                        "/list" => {
                            let word_arr = load_words(&message.from.id, &collection).await.unwrap();
                            let user_word_arr = WordsUserFriendly::new(&word_arr);
                            api.send(chat.text(format!("You list: {}", user_word_arr))).await?;
                        }
                        "/help" => {
                            api.send(chat.text(COMMAND_LIST)).await?;
                        }
                        "/random" => {
                            let word = WordsUserFriendly::new(&vec!(random_reminder(&message.from, &collection).await.unwrap()));
                            api.send(chat.text(format!("{}", word))).await?;
                        }
                        "/new" => {
                            api.send(chat.text("Please send /new <new Word> command format")).await?;
                        }
                        "/clear" => {
                            let words = WordsUserFriendly::new(&clear_words(&message.from, &collection).await.unwrap());
                            api.send(chat.text(format!("Done! \nYou list:  {}", words))).await?;
                        }
                        _ => {
                            api.send(chat.text(format!("Please send correct command from list: \n{}", COMMAND_LIST))).await?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn connect_to_db() -> Result<Collection, Error> {
    let client_options = ClientOptions::parse("mongodb+srv://mshassium:6308280156mng@cluster0.tndjw.mongodb.net").await?;
    let client = Client::with_options(client_options)?;
    let db = client.database("live_reminder");
    let collection = db.collection("user_words");
    Ok(collection)
}


async fn load_words(user_id: &UserId, collection: &Collection) -> Result<Array, Error> {
    let mut cursor = collection.find(doc! {"user_id":user_id.to_string()}, None).await?;
    let mut res_arr: Array = vec![];
    while let Some(result) = cursor.next().await {
        match result {
            Ok(document) => {
                if let Some(words) = document.get("words").and_then(Bson::as_array) {
                    println!("[DEBUG]------> Words: {:?}", words);
                    res_arr = words.to_vec();
                } else {
                    println!("[DEBUG]------> no words found");
                }
            }
            Err(_e) => {}
        }
    }
    Ok(res_arr)
}

async fn save_word(user: &User, new_word: &str, collection: &Collection) -> Result<Bson, Error> {
    let mut options = FindOneAndUpdateOptions::default();
    options.upsert = Some(true);
    options.return_document = Some(ReturnDocument::After);
    let res = collection.find_one_and_update(
        doc! {"user_id":user.id.to_string()},
        doc! {"$push":{"words":{"$each":[new_word]}}, "$set":{"name":user.first_name.as_str()}},
        options,
    ).await?;
    println!("[DEBUG]------> Save operation result: {:?}", res);
    Ok(bson::to_bson(&res).unwrap())
}

struct WordsUserFriendly {
    words: Vec<Bson>
}

impl WordsUserFriendly {
    fn new(arr: &Array) -> WordsUserFriendly {
        WordsUserFriendly {
            words: arr.clone()
        }
    }
}

impl fmt::Display for WordsUserFriendly {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "\n--------------------------\n")?;
        for v in &self.words {
            write!(f, "{}", v)?;
            write!(f, "\n")?;
        }
        write!(f, "--------------------------")
    }
}

async fn clear_words(user: &User, collection: &Collection) -> Result<Array, Error> {
    let user_id: &str = &user.id.to_string();
    println!("[DEBUG]------> clear words for : {}", user_id);
    collection.find_one_and_delete(doc! {"user_id":user_id}, None).await?;
    load_words(&user.id, collection).await
}

async fn random_reminder(user: &User, collection: &Collection) -> Result<Bson, Error> {
    let vec: Vec<Bson> = load_words(&user.id, collection).await.unwrap();
    let mut rng = rand::thread_rng();
    let option = vec.choose(&mut rng).unwrap().clone();
    println!("[DEBUG]------> For user {:?} choose word {:?}", user.id.to_string(), option);
    Ok(option)
}
