use futures::{StreamExt};
use telegram_bot::*;
use mongodb::{sync::Client, sync::Collection, bson::{doc, Bson, Array}, bson};
use mongodb::error::Error;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument, FindOptions, FindOneOptions};
use core::fmt;
use std::fmt::Formatter;
use rand::seq::SliceRandom;
use async_await::{thread};
use job_scheduler::{JobScheduler, Job};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use stoppable_thread::StoppableHandle;
use std::borrow::{Borrow, BorrowMut};
use std::time::SystemTime;
use serde::export::TryFrom;

const DEBUG_LOG: &str = "[DEBUG]------>";
const RELEASE_BOT_TOKEN: &str = "1218027891:AAE40Ml4He8_2gHqTOCtNOB3k5Dj2g1NgqQ";
const TEST_BOT_TOKEN: &str = "1328882225:AAEzOZOeZ6w1uO3o7ugBybSu7FsryWYt-U0";
const TZ_API_KEY: &str = "PRG4062PTQJU";
const HELP_PLACEHOLDER: &str = "\
Hello my friend ✌
This bot helps you to enjoy your life and remember the most important things ☺️
You can:
🍏 Add new importance phrase for your list (/new <long or short phrase>)
🍏 Remove phrase (/remove <the exact phrase to be deleted>)
🍏 Get list your phrase (/list)
🍏 Get random phrase from list (/random)
🍏 Update your location to adjust the reminder schedule (/location)
🍏 Schedule concrete time (/schedule <time list>)
    |
    |
    - - -> Example: /schedule 9,12,15,21,23
🍏 Clear list (/clear)
🍏 Show help message (/help)

❗️❗️❗️Feel free to send me any feedback please (@rail_khamitov)
";


#[tokio::main]
async fn main() -> Result<(), telegram_bot::Error> {
    println!("[DEBUG]------> Application Started");
    let collection: Collection = connect_to_db();
    let api: Api = init_api();
    send_hello_notification(false, &api, &collection).await;
    let mut threads: HashMap<String, StoppableHandle<()>> = reminder_logic(&collection);
    println!("[DEBUG]------> Reminder Logic Initialized");
    message_logic(&api, &collection, &mut threads).await.unwrap();
    println!("[DEBUG]------> Application Stopped");
    Ok(())
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

📝 Release Notes:
🍏 Add custom time for reminder for each user (send command /schedule <time list>)
    |
    |
    - - -> Example: /schedule 9,12,15,21,23
📌 9,21 - means that the bot will send 2 reminders at 9 and 21 hours every day

➡️ Here's what we plan to do in the near future:
🍎 Add support image/sticker/video for your list

            ", env!("CARGO_PKG_VERSION"));
            let res = api.send(chat.text(hello_notification)).await;
            match res {
                Ok(_r) => {}
                Err(e) => { println!("[DEBUG]------> ERROR ------> we can not send notification for user: {} because: {}", user_id, e) }
            }
        }
    }
}

async fn send_reminders(user_id: String, api: &Api, collection: &Collection) -> Result<(), Error> {
    println!("[DEBUG]------> In to send_reminder function");
    let chat = ChatId::new(user_id.parse::<i64>().unwrap());
    println!("[DEBUG]------> For user_id {} send reminder", user_id);
    api.send(chat.text(format!("{}", WordsUserFriendly::from_str(random_reminder(user_id, collection).unwrap().as_str().unwrap())))).await.unwrap();
    Ok(())
}

async fn message_logic(api: &Api,
                       collection: &Collection,
                       reminder_threads: &mut HashMap<String, StoppableHandle<()>>) -> Result<(), Error> {
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
                    let new_phrase = &data.as_str()[4..].trim();//4 because need remove first char '/new'
                    if new_phrase.len() > 300 {
                        api.send(chat.text(format!("☺️ Your message is too long. Max length = 300 characters"))).await.unwrap();
                    } else {
                        println!("[DEBUG]------> clear_word_string: {}", new_phrase);
                        let new_word_list = save_word(&message.from, new_phrase, collection).unwrap();
                        let new_words_arr = WordsUserFriendly::new(new_word_list.as_document().unwrap().get_array("words").unwrap());
                        api.send(chat.text(format!("I save your word ☺️ \nYour new word list 📋: {} ", new_words_arr))).await.unwrap();
                    }
                } else if data.as_str().starts_with("/remove ") {
                    let phrase_to_remove = data.as_str()[7..].trim();//4 because need remove first char '/new'
                    if phrase_to_remove.len() > 300 {
                        api.send(chat.text(format!("☺️ You do not have this phrase "))).await.unwrap();
                    } else {
                        println!("[DEBUG]------> delete phrase: {}", phrase_to_remove);
                        let new_word_list = remove_phrase(&message.from.id.to_string(), &collection, phrase_to_remove).unwrap();
                        api.send(chat.text(format!("Done 📗 \nYour new word list 📋: {} ", WordsUserFriendly::new(&new_word_list)))).await.unwrap();
                    }
                } else if data.as_str().starts_with("/schedule ") {
                    schedule_command(data, &message.from.id, api, collection, reminder_threads).await;
                } else {
                    match data.as_str() {
                        "/list" => {
                            let word_arr = load_words(&message.from.id.to_string(), collection).unwrap();
                            let user_word_arr = WordsUserFriendly::new(&word_arr);
                            api.send(chat.text(format!("{}", user_word_arr))).await.unwrap();
                        }
                        "/help" => {
                            api.send(chat.text(HELP_PLACEHOLDER)).await.unwrap();
                        }
                        "/random" => {
                            let word = WordsUserFriendly::new(&vec!(random_reminder(message.from.id.to_string(), collection).unwrap()));
                            api.send(chat.text(format!("{}", word))).await.unwrap();
                        }
                        "/new" => {
                            api.send(chat.text("❗️ Please send /new <new Word> command format")).await.unwrap();
                        }
                        "/clear" => {
                            let words = WordsUserFriendly::new(&clear_words(&message.from.id.to_string(), collection).unwrap());
                            api.send(chat.text(format!("Done! \nYour list 📋:  {}", words))).await.unwrap();
                        }
                        "/location" => {
                            api.send(chat.text("📍 Okay, please send me you location \n⚠️ Only from mobile app. \n\nIf you are worried about the security of your address, you can send any other location close to you. We only need this information to determine your time zone")).await.unwrap();
                        }
                        "/reminder_test" => {
                            send_reminders(message.from.id
                                               .to_string(), &api, &collection).await.unwrap();
                        }
                        _ => {
                            api.send(chat.text(format!("Please send correct command from list 📋: \n{}", HELP_PLACEHOLDER))).await.unwrap();
                        }
                    }
                }
            } else if
            let MessageKind::Location { data } = message.kind {
                println!("[DEBUG]------> user {} send location lat: {}, long: {} ", message.from.id, data.latitude, data.longitude);
                let get_timezone_url: String = format!("http://api.timezonedb.com/v2.1/get-time-zone?key=PRG4062PTQJU&format=json&by=position&lat={}&lng={}", data.latitude, data.longitude);
                let get_timezone_res = reqwest::get(&get_timezone_url)
                    .await.unwrap()
                    .text()
                    .await.unwrap();
                println!("[DEBUG]------> Location data res: {}", get_timezone_res);
                let time_zone_raw_data: TimeZoneRawData = serde_json::from_str(get_timezone_res.as_str()).unwrap();
                save_location(&message.from, time_zone_raw_data.zone_name.as_str(), &collection).unwrap();
                api.send(message.chat.text(format!("Thank you.☺️ \nWe are saved timezone data {}. \nNow you can configure required schedule for reminders", time_zone_raw_data.zone_name))).await.unwrap();
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

fn init_api() -> Api {
    Api::new(TEST_BOT_TOKEN)
}

fn reminder_logic(collection: &Collection) -> HashMap<String, StoppableHandle<()>> {
    let mut opt = FindOptions::default();
    opt.projection = Some(doc! {"user_id":true});
    let user_ids: Vec<String> = get_user_ids(collection);
    let user_times_arc = Arc::new(get_user_times(&collection, &user_ids));
    let mut threads: HashMap<String, StoppableHandle<()>> = HashMap::new();
    for user_id in user_ids {
        if user_times_arc.contains_key(&user_id) {
            let user_time = user_times_arc.get(&user_id).unwrap();
            let new_thread = schedule_reminder_for_concrete_user(&user_id, user_time, &mut threads, &collection);
            threads.insert(user_id, new_thread);
        }
    }
    println!("{} Reminder logic result {:?}", DEBUG_LOG, threads.len());
    threads
}

fn schedule_reminder_for_concrete_user(user_id: &String,
                                       user_time: &Vec<String>,
                                       threads: &mut HashMap<String, StoppableHandle<()>>,
                                       collection: &Collection) -> StoppableHandle<()> {
    let user_id_temp = String::from(user_id);
    println!("{:?}", user_time);
    let concrete_times = Arc::new(user_time.clone()).clone();
    if threads.contains_key(&user_id_temp) {
        println!("{} stop thread for user_id: {}", DEBUG_LOG, &user_id_temp);
    }
    stoppable_thread::spawn(move |_| {
        let collection: Collection = connect_to_db();
        let api: Api = init_api();
        println!("[DEBUG]------> INTO Reminder Thread for user_id: {} and times: {:?}", user_id_temp, concrete_times);
        let result_hours = concrete_times.join(",");
        println!("{} Joined time: {}", DEBUG_LOG, result_hours);
        let result_schedule_config = format!("0 0 {} * * *", result_hours);
        println!("{} Result schedule config: {}", DEBUG_LOG, result_hours);
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let mut sched = JobScheduler::new();
        sched.add(Job::new(result_schedule_config.parse().unwrap(), move || {
            let _block = rt.block_on(send_reminders(String::from(&user_id_temp), &api, &collection));
        }));
        loop {
            sched.tick();
        }
    })
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

fn get_user_times(collection: &Collection, user_ids: &Vec<String>) -> HashMap<String, Vec<String>> {
    println!("[DEBUG]------> In to get_user_times fn");
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    for user_id in user_ids {
        collection.find_one(doc! {"user_id":user_id}, None)
            .unwrap()
            .map(|res| {
                let doc: bson::Document = res;
                let default_times = &Bson::from(vec!["5", "17"]);
                let next_arr = doc.get("reminder_time")
                    .unwrap_or(default_times)
                    .as_array()
                    .unwrap();
                let res_arr = next_arr.iter()
                    .map(|item| {
                        item.as_str().unwrap().to_string()
                    }).collect::<Vec<String>>();
                result.insert(user_id.to_lowercase(), res_arr);
            });
    }
    println!("[DEBUG]------> result: {:?}", result);
    result
}

async fn convert_timezone(user_id: &String, raw_times: &Vec<String>, collection: &Collection) -> Result<Vec<String>, Error> {
    println!("{} convert_timezone fn for user_id: {}", DEBUG_LOG, user_id);
    let user_timezone = String::from(collection.find_one(doc! {"user_id":user_id}, None)
        .unwrap()
        .map(|res| {
            let res_doc: bson::Document = res;
            let def_timezone = &mut Bson::from("UTC");
            let res_bson = res_doc.get("timezone")
                .unwrap_or(def_timezone);
            res_bson.clone()
        }).unwrap().as_str().unwrap());
    let local_timezone = "UTC";
    let convert_timezone_url: String = format!("http://api.timezonedb.com/v2.1/convert-time-zone?key=PRG4062PTQJU&format=json&from={}&to={}&time={}",
                                               user_timezone,
                                               local_timezone,
                                               "1598214923");
    let convert_time_res = reqwest::get(&convert_timezone_url)
        .await.unwrap()
        .text()
        .await.unwrap();
    let tz: ConvertedTimeZone = serde_json::from_str(&convert_time_res).unwrap();
    println!("[DEBUG]------> Convert timezone data res: {}", convert_time_res);
    let offset_hour = tz.offset / 3600;
    println!("[DEBUG]------> offset_hour: {}", offset_hour);
    let converted_times = raw_times.iter()
        .map(|next_time| {
            let next_time_int = next_time.parse::<i64>().unwrap();
            (next_time_int + offset_hour).to_string()
        }).collect::<Vec<String>>();
    println!("[DEBUG]------> converted_times: {:?}", converted_times);
    Ok(converted_times)
}

fn save_location(user: &User, timezone: &str, collection: &Collection) -> Result<Bson, Error> {
    println!("[DEBUG]------> Start save location");
    let mut options = FindOneAndUpdateOptions::default();
    options.upsert = Some(true);
    options.return_document = Some(ReturnDocument::After);
    let res = collection.find_one_and_update(
        doc! {"user_id":user.id.to_string()},
        doc! {"$set":{"timezone":timezone}},
        options,
    ).unwrap();
    println!("[DEBUG]------> Save operation result: {:?}", res);
    Ok(bson::to_bson(&res).unwrap())
}

fn clear_words(user_id: &String, collection: &Collection) -> Result<Array, Error> {
    println!("[DEBUG]------> clear words for : {}", user_id);
    collection.find_one_and_delete(doc! {"user_id":user_id}, None).unwrap();
    load_words(user_id, collection)
}

fn remove_phrase(user_id: &String, collection: &Collection, phrase: &str) -> Result<Array, Error> {
    println!("[DEBUG]------> remove_phrase {} for : {}", phrase, user_id);
    collection.find_one_and_update(doc! {"user_id":user_id},
                                   doc! {"$pull":{"words":phrase}},
                                   None)
        .unwrap();
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
    println!("[DEBUG]------> user_ids size: {}", user_ids.len());
    user_ids
}

fn timezone_available(user_id: &UserId, collection: &Collection) -> bool {
    println!("[DEBUG]------> timezone_available fn for user_id: {}", user_id);
    let mut opt = FindOptions::default();
    opt.projection = Some(doc! {"timezone":true});
    let res = collection.find(doc! {"user_id":user_id.to_string()}, opt)
        .unwrap()
        .map(|res| {
            let doc: bson::Document = res.unwrap();
            return doc
                .get("timezone")
                .unwrap_or(&Bson::from(""))
                .as_str()
                .unwrap()
                .to_string();
        }).collect::<Vec<String>>();
    println!("[DEBUG]------> res vec size {} for user_id: {}", res.len(), user_id, );
    if res.len() > 0 && res[0] != "" {
        true
    } else {
        false
    }
}

async fn schedule_command(data: &String,
                          user_id: &UserId,
                          api: &Api,
                          collection: &Collection,
                          threads: &mut HashMap<String, StoppableHandle<()>>) {
    println!("[DEBUG]------> schedule_command");
    let schedule_times_string = data
        .as_str()[9..]
        .trim()
        .split(",")
        .filter(|t| t.parse::<i32>().unwrap() > 0 && t.parse::<i32>().unwrap() < 23)
        .map(|time| {
            time.trim().to_string()
        })
        .collect::<Vec<String>>();
    println!("[DEBUG]------> schedule_times_string: {:?}", schedule_times_string);
    if timezone_available(user_id, &collection) {
        if let res = push_new_reminder_time(user_id, &collection, &schedule_times_string).await.unwrap() {
            if res {
                let res_thread = schedule_reminder_for_concrete_user(&user_id.to_string(),
                                                                     &schedule_times_string,
                                                                     threads,
                                                                     &collection);
                threads.insert(user_id.to_string(), res_thread);
                api.send(user_id.text(format!("👍 Time for reminders changed on {:?}", schedule_times_string))).await.unwrap();
            } else {
                api.send(user_id.text(format!("😔 Sorry, we can't change reminder time."))).await.unwrap();
                ()
            }
        }
    } else {
        api.send(user_id.text("We can't set your schedule time because we do not know your location \nPlease specify location information (try /location)")).await.unwrap();
    }
}

async fn push_new_reminder_time(user_id: &UserId, collection: &Collection, times: &Vec<String>) -> Result<bool, Error> {
    println!("[DEBUG]------> push_new_reminder_time for user: {}", user_id);
    let mut options = FindOneAndUpdateOptions::default();
    options.upsert = Some(false);
    options.return_document = Some(ReturnDocument::After);
    let converted_time = convert_timezone(&user_id.to_string(), times, &collection).await.unwrap();
    collection.find_one_and_update(doc! {"user_id":user_id.to_string()},
                                   doc! {"$set":{"reminder_time":[]}},
                                   None).unwrap();
    match collection.find_one_and_update(doc! {"user_id":user_id.to_string()},
                                         doc! {"$push":{"reminder_time":{"$each":converted_time}}},
                                         options) {
        Ok(document) => Ok(true),
        Err(e) => {
            println!("[DEBUG]------> Error {:?}", e);
            Ok(false)
        }
    }
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
    fn from_str(single_str: &str) -> WordsUserFriendly {
        WordsUserFriendly {
            words: vec![bson::Bson::from(single_str)]
        }
    }
}

impl fmt::Display for WordsUserFriendly {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "\n🔸🔸🔸🔸🔸🔸🔸🔸🔸\n\n")?;
        for (pos, v) in self.words.iter().enumerate() {
            write!(f, "{}) {}", pos + 1, v)?;
            write!(f, "\n\n")?;
        }
        write!(f, "🔸🔸🔸🔸🔸🔸🔸🔸🔸")
    }
}

#[derive(Serialize, Deserialize)]
struct TimeZoneRawData {
    status: String,
    message: String,
    #[serde(rename = "countryCode")]
    country_code: String,
    #[serde(rename = "countryName")]
    country_name: String,
    #[serde(rename = "zoneName")]
    zone_name: String,
    timestamp: u64,
    formatted: String,
}

#[derive(Serialize, Deserialize)]
struct ConvertedTimeZone {
    status: String,
    #[serde(rename = "fromZoneName")]
    from_zone_name: String,
    #[serde(rename = "toZoneName")]
    to_zone_name: String,
    #[serde(rename = "fromTimestamp")]
    from_timestamp: u64,
    #[serde(rename = "toTimestamp")]
    to_timestamp: u64,
    offset: i64,
}
