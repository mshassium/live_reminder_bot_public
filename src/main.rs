mod model;

use futures::{StreamExt};
use telegram_bot::*;
use mongodb::{sync::Client, sync::Collection, bson::{doc, Bson, Array}, bson};
use mongodb::error::Error;
use mongodb::options::{FindOneAndUpdateOptions, ReturnDocument, FindOptions};
use rand::seq::SliceRandom;
use job_scheduler::{JobScheduler, Job};
use std::collections::HashMap;
use std::sync::{Arc};
use stoppable_thread::StoppableHandle;
use model::UserPhrase;
use crate::model::{ConvertedTimeZone, TimeZoneRawData};

const RELEASE_BOT_TOKEN: &str = "";
const DB_CREDENTIAL: &str = "";
const TZ_API_KEY: &str = "";
const MAX_MESSAGE_LEN: usize = 300;

//[TODO] refactoring long message to read file
const HELP_PLACEHOLDER: &str = "\
Hello my friend ✌
This bot helps you to enjoy your life and remember the most important things ☺️
You can:
1️⃣  Add new importance phrase for your list (/new <long or short phrase>)
2️⃣  Remove phrase (/remove <the exact phrase to be deleted>)
3️⃣  Get list your phrase (/list)
4️⃣  Get random phrase from list (/random)
5️⃣  Update your location to adjust the reminder schedule (/location)
6️⃣  Schedule concrete time (/schedule <time list>)
     |
      --> Example: /schedule 9,12,15,21,23
7️⃣ Clear list (/clear)
8️⃣ Show help message (/help)

❗️❗️❗️Feel free to send me any feedback please (@rail_khamitov)
";


#[tokio::main]
async fn main() -> Result<(), telegram_bot::Error> {
    println!("[DEBUG]------> Application Started");
    let collection: Collection = connect_to_db();
    let api: Api = init_telegram_api();
    send_hello_notification(false, &api, &collection).await;
    let mut threads: HashMap<String, StoppableHandle<()>> = schedule_reminders_for_all_users(&collection);
    println!("[DEBUG]------> Reminder Logic Initialized");
    message_loop(&api, &collection, &mut threads).await.unwrap();
    println!("[DEBUG]------> Application Stopped");
    Ok(())
}

fn connect_to_db() -> Collection {
    println!("[DEBUG]------> DB Connection Start");
    let client = Client::with_uri_str(format!("mongodb+srv://{}@cluster0.tndjw.mongodb.net", DB_CREDENTIAL).as_str()).unwrap();
    let db = client.database("live_reminder");
    let collection = db.collection("user_words");
    println!("[DEBUG]------> DB Connection DONE");
    collection
}

fn init_telegram_api() -> Api {
    Api::new(RELEASE_BOT_TOKEN)
}

async fn send_hello_notification(send: bool, api: &Api, collection: &Collection) {
    println!("[DEBUG]------> Into send_hello_notification method");
    if send {
        println!("[DEBUG]------> Hello notification send");
        let user_ids = get_user_ids(collection);
        for user_id in user_ids {
            let chat = ChatId::new(user_id.parse::<i64>().unwrap());
            println!("[DEBUG]------> For user_id {} send hello notification", user_id);
            //[TODO] refactoring long message to read file
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

async fn send_random_phrase_for_user(user_id: String,
                                     api: &Api,
                                     collection: &Collection) -> Result<(), Error> {
    println!("[DEBUG]------> In send_random_phrase_for_user send_reminder function");
    let chat = ChatId::new(user_id.parse::<i64>().unwrap());
    println!("[DEBUG]------> For user_id {} send reminder", user_id);
    api.send(chat
        .text(
            format!("{}",
                    UserPhrase::from_str(get_random_phrase(user_id, collection)
                        .unwrap()
                        .as_str()
                        .unwrap()
                    )
            )
        )
    ).await.unwrap();
    Ok(())
}

async fn message_loop(api: &Api,
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
                        let new_word_list = save_new_phrase(&message.from, new_phrase, collection).unwrap();
                        let new_words_arr = UserPhrase::new(new_word_list.as_document().unwrap().get_array("words").unwrap());
                        api.send(chat.text(format!("I save your word ☺️ \nYour new word list 📋: {} ", new_words_arr))).await.unwrap();
                    }
                } else if data.as_str().starts_with("/remove ") {
                    let phrase_to_remove = data.as_str()[7..].trim();//4 because need remove first char '/new'
                    if phrase_to_remove.len() > MAX_MESSAGE_LEN {
                        api.send(chat.text(format!("☺️ You do not have this phrase "))).await.unwrap();
                    } else {
                        println!("[DEBUG]------> delete phrase: {}", phrase_to_remove);
                        let new_word_list = remove_phrase(&message.from.id.to_string(), &collection, phrase_to_remove).unwrap();
                        api.send(chat.text(format!("Done 📗 \nYour new word list 📋: {} ", UserPhrase::new(&new_word_list)))).await.unwrap();
                    }
                } else if data.as_str().starts_with("/schedule ") {
                    parse_schedule_command(data, &message.from.id, api, collection, reminder_threads).await;
                } else {
                    match data.as_str() {
                        "/list" => {
                            let word_arr = get_phrases(&message.from.id.to_string(), collection).unwrap();
                            let user_word_arr = UserPhrase::new(&word_arr);
                            api.send(chat.text(format!("{}", user_word_arr))).await.unwrap();
                        }
                        "/help" => {
                            api.send(chat.text(HELP_PLACEHOLDER)).await.unwrap();
                        }
                        "/random" => {
                            let word = UserPhrase::new(&vec!(get_random_phrase(message.from.id.to_string(), collection).unwrap()));
                            api.send(chat.text(format!("{}", word))).await.unwrap();
                        }
                        "/new" => {
                            api.send(chat.text("❗️ Please send /new <new Word> command format")).await.unwrap();
                        }
                        "/clear" => {
                            let words = UserPhrase::new(&remove_all_phrases(&message.from.id.to_string(), collection).unwrap());
                            api.send(chat.text(format!("Done! \nYour list 📋:  {}", words))).await.unwrap();
                        }
                        "/location" => {
                            api.send(chat.text("📍 Okay, please send me you location \n⚠️ Only from mobile app. \n\nIf you are worried about the security of your address, you can send any other location close to you. We only need this information to determine your time zone")).await.unwrap();
                        }
                        "/reminder_test" => {
                            send_random_phrase_for_user(message.from.id
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
                let get_timezone_url: String = format!("http://api.timezonedb.com/v2.1/get-time-zone?key={}&format=json&by=position&lat={}&lng={}", TZ_API_KEY, data.latitude, data.longitude);
                let get_timezone_res = reqwest::get(&get_timezone_url)
                    .await.unwrap()
                    .text()
                    .await.unwrap();
                println!("[DEBUG]------> Location data res: {}", get_timezone_res);
                let time_zone_raw_data: TimeZoneRawData = serde_json::from_str(get_timezone_res.as_str()).unwrap();
                save_user_location(&message.from, time_zone_raw_data.zone_name.as_str(), &collection).unwrap();
                api.send(message.chat.text(format!("Thank you.☺️ \nWe are saved timezone data {}. \nNow you can configure required schedule for reminders", time_zone_raw_data.zone_name))).await.unwrap();
            }
        }
    }
    Ok(())
}

fn schedule_reminders_for_all_users(collection: &Collection) -> HashMap<String, StoppableHandle<()>> {
    let mut opt = FindOptions::default();
    opt.projection = Some(doc! {"user_id":true});
    let user_ids: Vec<String> = get_user_ids(collection);
    let user_times_arc = Arc::new(get_user_times(&collection, &user_ids));
    let mut threads: HashMap<String, StoppableHandle<()>> = HashMap::new();
    for user_id in user_ids {
        if user_times_arc.contains_key(&user_id) {
            let user_time = user_times_arc.get(&user_id).unwrap();
            let new_thread = schedule_reminder_for_concrete_user(&user_id, user_time, &mut threads);
            threads.insert(user_id, new_thread);
        }
    }
    println!("[DEBUG]------> Reminder logic result {:?}", threads.len());
    threads
}

fn schedule_reminder_for_concrete_user(user_id: &String,
                                       user_time: &Vec<String>,
                                       threads: &mut HashMap<String, StoppableHandle<()>>) -> StoppableHandle<()> {
    let user_id_temp = String::from(user_id);
    println!("[DEBUG]------> schedule_reminder_for_concrete_user: {}, time: {:?}", user_id, user_time);
    let concrete_times = Arc::new(user_time.clone()).clone();
    if threads.contains_key(&user_id_temp) {
        println!("[DEBUG]------> stop thread for user_id: {}", &user_id_temp);
    }
    stoppable_thread::spawn(move |_| {
        let collection: Collection = connect_to_db();
        let api: Api = init_telegram_api();
        println!("[DEBUG]------> INTO Reminder Thread for user_id: {} and times: {:?}", user_id_temp, concrete_times);
        let result_hours = concrete_times.join(",");
        println!("[DEBUG]------> Joined time: {}", result_hours);
        let result_schedule_config = format!("0 0 {} * * *", result_hours);
        println!("[DEBUG]------> Result schedule config: {}", result_hours);
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let mut sched = JobScheduler::new();
        sched.add(Job::new(result_schedule_config.parse().unwrap(), move || {
            let _block = rt.block_on(send_random_phrase_for_user(String::from(&user_id_temp), &api, &collection));
        }));
        loop {
            sched.tick();
        }
    })
}

fn get_phrases(user_id: &String, collection: &Collection) -> Result<Array, Error> {
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

fn get_random_phrase(user_id: String, collection: &Collection) -> Result<Bson, Error> {
    let vec: Vec<Bson> = get_phrases(&user_id, collection).unwrap();
    if vec.len() > 0 {
        let mut rng = rand::thread_rng();
        let option = vec.choose(&mut rng).unwrap().clone();
        println!("[DEBUG]------> For user {:?} choose phrase {:?}", user_id, option);
        Ok(option)
    } else {
        println!("[DEBUG]------> Empty list");
        Ok(bson::to_bson("Empty list").unwrap())
    }
}


async fn parse_schedule_command(data: &String,
                                user_id: &UserId,
                                api: &Api,
                                collection: &Collection,
                                threads: &mut HashMap<String, StoppableHandle<()>>) {
    println!("[DEBUG]------> parse_schedule_command : {}", user_id.to_string());
    let schedule_times_string = data
        .as_str()[9..]
        .trim()
        .split(",")
        .filter(|t| t.trim().parse::<i32>().unwrap() > 0 && t.trim().parse::<i32>().unwrap() < 23)
        .map(|time| {
            time.trim().to_string()
        })
        .collect::<Vec<String>>();
    println!("[DEBUG]------> schedule_times_string: {:?}", schedule_times_string);
    if timezone_is_available(user_id, &collection) {
        if save_new_reminder_time(user_id, &collection, &schedule_times_string).await.unwrap() {
            let res_thread = schedule_reminder_for_concrete_user(&user_id.to_string(),
                                                                 &schedule_times_string,
                                                                 threads);
            threads.insert(user_id.to_string(), res_thread);
            api.send(user_id.text(format!("👍 Time for reminders changed on {:?}", schedule_times_string))).await.unwrap();
        } else {
            api.send(user_id.text(format!("😔 Sorry, we can't change reminder time."))).await.unwrap();
            ()
        }
    } else {
        api.send(user_id.text("We can't set your schedule time because we do not know your location \nPlease specify location information (try /location)")).await.unwrap();
    }
}

async fn convert_user_to_system_timezones(user_id: &String, raw_times: &Vec<String>, collection: &Collection) -> Result<Vec<String>, Error> {
    println!("[DEBUG]------> convert_timezone fn for user_id: {}", user_id);
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

fn save_user_location(user: &User, timezone: &str, collection: &Collection) -> Result<Bson, Error> {
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

fn save_new_phrase(user: &User, new_word: &str, collection: &Collection) -> Result<Bson, Error> {
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

async fn save_new_reminder_time(user_id: &UserId, collection: &Collection, times: &Vec<String>) -> Result<bool, Error> {
    println!("[DEBUG]------> save_new_reminder_time for user: {}", user_id);
    let mut options = FindOneAndUpdateOptions::default();
    options.upsert = Some(false);
    options.return_document = Some(ReturnDocument::After);
    let converted_time = convert_user_to_system_timezones(&user_id.to_string(), times, &collection).await.unwrap();
    collection.find_one_and_update(doc! {"user_id":user_id.to_string()},
                                   doc! {"$set":{"reminder_time":[]}},
                                   None).unwrap();
    match collection.find_one_and_update(doc! {"user_id":user_id.to_string()},
                                         doc! {"$push":{"reminder_time":{"$each":converted_time}}},
                                         options) {
        Ok(_document) => Ok(true),
        Err(e) => {
            println!("[DEBUG]------> Error {:?}", e);
            Ok(false)
        }
    }
}

fn remove_all_phrases(user_id: &String, collection: &Collection) -> Result<Array, Error> {
    println!("[DEBUG]------> remove_all_phrases for : {}", user_id);
    collection.find_one_and_delete(doc! {"user_id":user_id}, None).unwrap();
    get_phrases(user_id, collection)
}

fn remove_phrase(user_id: &String, collection: &Collection, phrase: &str) -> Result<Array, Error> {
    println!("[DEBUG]------> remove_phrase {} for : {}", phrase, user_id);
    collection.find_one_and_update(doc! {"user_id":user_id},
                                   doc! {"$pull":{"words":phrase}},
                                   None)
        .unwrap();
    get_phrases(user_id, collection)
}

fn timezone_is_available(user_id: &UserId, collection: &Collection) -> bool {
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



