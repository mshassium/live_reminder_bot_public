use futures::{StreamExt};
use telegram_bot::*;
use mongodb::{sync::Client, sync::Collection,  bson::{doc, Bson, Array}, bson};
use mongodb::error::Error;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument, FindOptions};
use core::fmt;
use std::fmt::Formatter;
use rand::seq::SliceRandom;
use async_await::thread;
use std::time::{Duration};
use job_scheduler::{JobScheduler, Job};


const COMMAND_LIST: &str = "/list \n/help \n/random \n/clear \n/new <word> ";
const BOT_TOKEN: &str = "1218027891:AAE40Ml4He8_2gHqTOCtNOB3k5Dj2g1NgqQ";

#[tokio::main]
async fn main() -> Result<(), telegram_bot::Error> {
    println!("[DEBUG]------> Application Started");
    reminder_logic();
    println!("[DEBUG]------> Reminder Logic Initialized");
    message_logic().await.unwrap();
    println!("[DEBUG]------> Application Stopped");
    Ok(())
}

fn reminder_logic() {
    thread::spawn(|| {
        println!("[DEBUG]------> INTO Reminder Thread");
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let mut sched = JobScheduler::new();
        let collection = connect_to_db().unwrap();
        let api = Api::new(BOT_TOKEN);
        sched.add(Job::new("* 1 9,21 * * *".parse().unwrap(), move || {
            let _block = rt.block_on(send_reminders(&api, &collection));
        }));
        loop {
            sched.tick();
            thread::sleep(Duration::from_secs(1800));
        }
    });
}

async fn message_logic() -> Result<(), Error> {
    println!("[DEBUG]------> INTO Message Logic Thread");
    let collection = connect_to_db().unwrap();
    let api = Api::new(BOT_TOKEN);
    // Fetch new updates via long poll method
    let mut stream = api.stream();
    println!("[DEBUG]------> Waiting Message...");
    while let Some(update) = stream.next().await {
        // If the received update contains a new message...
        let update = update.unwrap();
        if let UpdateKind::Message(message) = update.kind {
            if let MessageKind::Text { ref data, ref entities } = message.kind {
                let chat = message.chat;
                println!("[DEBUG]------> <{}>: data: {} , entities: {:?}", &message.from.id, data, entities);
                if data.as_str().starts_with("/new ") {
                    let clear_word_string = &data.as_str()[4..].trim();//4 because need remove first char '/new'
                    println!("[DEBUG]------> clear_word_string: {}", clear_word_string);
                    let new_word_list = save_word(&message.from, clear_word_string, &collection).unwrap();
                    let new_words_arr = WordsUserFriendly::new(new_word_list.as_document().unwrap().get_array("words").unwrap());
                    api.send(chat.text(format!("I save you word:) \nYou new word list: {} ", new_words_arr))).await.unwrap();
                } else {
                    match data.as_str() {
                        "/list" => {
                            let word_arr = load_words(&message.from.id.to_string(), &collection).unwrap();
                            let user_word_arr = WordsUserFriendly::new(&word_arr);
                            api.send(chat.text(format!("Your list: {}", user_word_arr))).await.unwrap();
                        }
                        "/help" => {
                            api.send(chat.text(COMMAND_LIST)).await.unwrap();
                        }
                        "/random" => {
                            let word = WordsUserFriendly::new(&vec!(random_reminder(message.from.id.to_string(), &collection).unwrap()));
                            api.send(chat.text(format!("{}", word))).await.unwrap();
                        }
                        "/new" => {
                            api.send(chat.text("Please send /new <new Word> command format")).await.unwrap();
                        }
                        "/clear" => {
                            let words = WordsUserFriendly::new(&clear_words(&message.from.id.to_string(), &collection).unwrap());
                            api.send(chat.text(format!("Done! \nYour list:  {}", words))).await.unwrap();
                        }
                        _ => {
                            api.send(chat.text(format!("Please send correct command from list: \n{}", COMMAND_LIST))).await.unwrap();
                        }
                    }
                }
            }
        }
    }
    Ok(())
}


fn connect_to_db() -> Result<Collection, Error> {
    println!("[DEBUG]------> DB Connection Start");
    let client = Client::with_uri_str("mongodb+srv://mshassium:6308280156mng@cluster0.tndjw.mongodb.net")?;
    let db = client.database("live_reminder");
    let collection = db.collection("user_words");
    println!("[DEBUG]------> DB Connection DONE");
    Ok(collection)
}

fn load_words(user_id: &String, collection: &Collection) -> Result<Array, Error> {
    let mut cursor = collection.find(doc! {"user_id":user_id}, None).unwrap();
    let mut res_arr: Array = vec![];
    while let Some(result) = cursor.next() {
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

fn save_word(user: &User, new_word: &str, collection: &Collection) -> Result<Bson, Error> {
    let mut options = FindOneAndUpdateOptions::default();
    options.upsert = Some(true);
    options.return_document = Some(ReturnDocument::After);
    let res = collection.find_one_and_update(
        doc! {"user_id":user.id.to_string()},
        doc! {"$push":{"words":{"$each":[new_word]}}, "$set":{"name":user.first_name.as_str()}},
        options,
    ).unwrap();
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

fn clear_words(user_id: &String, collection: &Collection) -> Result<Array, Error> {
    println!("[DEBUG]------> clear words for : {}", user_id);
    collection.find_one_and_delete(doc! {"user_id":user_id}, None).unwrap();
    load_words(user_id, collection)
}

fn random_reminder(user_id: String, collection: &Collection) -> Result<Bson, Error> {
    let vec: Vec<Bson> = load_words(&user_id, collection).unwrap();
    if vec.len() > 0 {
        let mut rng = rand::thread_rng();
        let option = vec.choose(&mut rng).unwrap().clone();
        println!("[DEBUG]------> For user {:?} choose word {:?}", user_id, option);
        Ok(option)
    } else {
        println!("[DEBUG]------> Empty list");
        Ok(bson::to_bson("Empty list").unwrap())
    }
}

async fn send_reminders(api: &Api, collection: &Collection) -> Result<(), Error> {
    println!("[DEBUG]------> In to send_reminder function");
    let mut opt = FindOptions::default();
    opt.projection = Some(doc! {"user_id":true});
    let user_ids: Vec<String> =
        collection
            .find(doc! {}, opt)
            .unwrap()
            .map(|res| {
                let doc: bson::Document = res.unwrap();
                return doc
                    .get("user_id")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string();
            })
            .collect::<Vec<String>>();
    for user_id in user_ids {
        let chat = ChatId::new(user_id.parse::<i64>().unwrap());
        println!("[DEBUG]------> For user_id {} send reminder", user_id);
        let word: String = random_reminder(user_id, collection).unwrap().to_string();
        api.send(chat.text(word)).await.unwrap();
    }
    Ok(())
}
