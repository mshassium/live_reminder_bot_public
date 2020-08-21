#![feature(const_fn)]

use futures::{StreamExt};
use telegram_bot::*;
use mongodb::{sync::Client, sync::Collection, bson::{doc, Bson, Array}, bson};
use mongodb::error::Error;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument, FindOptions};
use core::fmt;
use std::fmt::Formatter;
use rand::seq::SliceRandom;
use async_await::{thread, collect};
use std::time::{Duration};
use job_scheduler::{JobScheduler, Job};


const COMMAND_LIST: &str = "/list \n/help \n/random \n/clear \n/new <word> ";
const RELEASE_BOT_TOKEN: &str = "1218027891:AAE40Ml4He8_2gHqTOCtNOB3k5Dj2g1NgqQ";
const TEST_BOT_TOKEN: &str = "1328882225:AAEzOZOeZ6w1uO3o7ugBybSu7FsryWYt-U0";
const HELP_PLACEHOLDER: &str = "\
Hello, my friend ✌
This bot help you for enjoy your life ☺️
You can:
🍏 Add new Importance word for your list (/new <word>)
🍏 Get list your importance words (/list)
🍏 Get random word from your list (/random)
🍏 Clear your list (/clear)
🍏 Show help message (/help)
❗️❗️❗️If you want send me any feedback please feel free (@rail_khamitov)
";


#[tokio::main]
async fn main() -> Result<(), telegram_bot::Error> {
    println!("[DEBUG]------> Application Started");
    let db_connection: Collection = connect_to_db();
    let api: Api = init_api();
    send_hello_notification(true, &api, &db_connection).await;
    reminder_logic();
    println!("[DEBUG]------> Reminder Logic Initialized");
    message_logic(&api, &db_connection).await.unwrap();
    println!("[DEBUG]------> Application Stopped");
    Ok(())
}

fn init_api() -> Api {
    Api::new(TEST_BOT_TOKEN)
}

fn reminder_logic() {
    thread::spawn(|| {
        let collection: Collection = connect_to_db();
        let api: Api = init_api();
        println!("[DEBUG]------> INTO Reminder Thread");
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let mut sched = JobScheduler::new();
        sched.add(Job::new("0 1 5,17 * * *".parse().unwrap(), move || {
            let _block = rt.block_on(send_reminders(&api, &collection));
        }));
        loop {
            sched.tick();
        }
    });
}

async fn send_hello_notification(send: bool, api: &Api, collection: &Collection) {
    println!("[DEBUG]------> Into send_hello_notification method");
    if send {
        println!("[DEBUG]------> Hello notification send");
        let user_ids = get_user_ids(collection);
        for user_id in user_ids {
            let chat = ChatId::new(user_id.parse::<i64>().unwrap());
            println!("[DEBUG]------> For user_id {} send hello notification", user_id);
            let hello_notification = format!("\
Hello!!!
We have some updates for you ☺️
Current bot version: {}

Release Notes:
🍏 Added this notification message
🍏 Added clear and useful description and help block for bot (try /help)

We are trying to develop this bot for you.
Here's what we plan to do in the near future:
🍎 Fix Timezone problem (Now all reminder send only for +04:00 Timezone)
🍎 Add custom time for reminder for each user (Now we send 2 reminders 9:00AM/PM )
🍎 Add availability to remove concrete word
🍎 Edit mode for concrete word
🍎 Add support image/sticker/video for your word list

            ",env!("CARGO_PKG_VERSION"));
            let res = api.send(chat.text(hello_notification)).await;
            match res {
                Ok(r) => {}
                Err(e) => { println!("[DEBUG]------> ERROR ------> we can not send notification for user: {}", user_id) }
            }
        }
    }
}

async fn message_logic(api: &Api, collection: &Collection) -> Result<(), Error> {
    println!("[DEBUG]------> INTO Message Logic Thread");
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
                    let new_word_list = save_word(&message.from, clear_word_string, collection).unwrap();
                    let new_words_arr = WordsUserFriendly::new(new_word_list.as_document().unwrap().get_array("words").unwrap());
                    api.send(chat.text(format!("I save your word:) \nYour new word list: {} ", new_words_arr))).await.unwrap();
                } else {
                    match data.as_str() {
                        "/list" => {
                            let word_arr = load_words(&message.from.id.to_string(), collection).unwrap();
                            let user_word_arr = WordsUserFriendly::new(&word_arr);
                            api.send(chat.text(format!("Your list: {}", user_word_arr))).await.unwrap();
                        }
                        "/help" => {
                            api.send(chat.text(HELP_PLACEHOLDER)).await.unwrap();
                        }
                        "/random" => {
                            let word = WordsUserFriendly::new(&vec!(random_reminder(message.from.id.to_string(), collection).unwrap()));
                            api.send(chat.text(format!("{}", word))).await.unwrap();
                        }
                        "/new" => {
                            api.send(chat.text("Please send /new <new Word> command format")).await.unwrap();
                        }
                        "/clear" => {
                            let words = WordsUserFriendly::new(&clear_words(&message.from.id.to_string(), collection).unwrap());
                            api.send(chat.text(format!("Done! \nYour list:  {}", words))).await.unwrap();
                        }
                        _ => {
                            api.send(chat.text(format!("Please send correct command from list: \n{}", HELP_PLACEHOLDER))).await.unwrap();
                        }
                    }
                }
            }
        }
    }
    Ok(())
}


fn connect_to_db() -> Collection {
    println!("[DEBUG]------> DB Connection Start");
    let client = Client::with_uri_str("mongodb+srv://mshassium:6308280156mng@cluster0.tndjw.mongodb.net").unwrap();
    let db = client.database("live_reminder");
    let collection = db.collection("user_words");
    println!("[DEBUG]------> DB Connection DONE");
    collection
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
    let user_ids: Vec<String> = get_user_ids(collection);
    for user_id in user_ids {
        let chat = ChatId::new(user_id.parse::<i64>().unwrap());
        println!("[DEBUG]------> For user_id {} send reminder", user_id);
        let word: String = random_reminder(user_id, collection).unwrap().to_string();
        api.send(chat.text(word)).await.unwrap();
    }
    Ok(())
}

fn get_user_ids(collection: &Collection) -> Vec<String> {
    println!("[DEBUG]------> In to get_user_ids function");
    let mut opt = FindOptions::default();
    opt.projection = Some(doc! {"user_id":true});
    let user_ids: Vec<String> = collection
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
    println!("[DEBUG]------> user_ids_ size: {}", user_ids.len());
    user_ids
}
