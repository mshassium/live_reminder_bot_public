// use futures::StreamExt;
use telegram_bot::*;
use mongodb::{
    bson::{doc, Bson},
    sync::Client,
};

// #[tokio::main]
fn main() -> Result<(), Error> {
    // let token = "1218027891:AAFXqS_nfC-hRp7WqQCnWMpYPyouZKiONhA";
    // let api = Api::new(token);

    // Fetch new updates via long poll method
    // let mut stream = api.stream();
    let client = Client::with_uri_str("mongodb+srv://mshassium:6308280156mongodb@cluster0.tndjw.mongodb.net").unwrap();
    let database = client.database("live_reminder");
    let collection = database.collection("user_words");
    let mut cursor = collection.find(doc! { "name": "rail" }, None).unwrap();
    println!("Cursor done {:?}", cursor.next().unwrap());
    // while let Some(update) = stream.next().await {
    //     // If the received update contains a new message...
    //     let update = update?;
    //     if let UpdateKind::Message(message) = update.kind {
    //         if let MessageKind::Text { ref data, .. } = message.kind {
    //             println!("<{}>: {}", &message.from.first_name, data);
    //         }
    //     }
    // }
    for result in cursor {
        match result {
            Ok(document) => {
                print!("{}",document);
                if let Some(title) = document.get("name").and_then(Bson::as_str) {
                    println!("title: {}", title);
                } else {
                    println!("no title found");
                }
            }
            Err(_e) => println!("Error"),
        }
    }
    Ok(())
}