use chrono::Local;
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::sync::Arc;
use teloxide::{
    dispatching::{Dispatcher, UpdateFilterExt},
    dptree,
    prelude::*,
    types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup},
};

/* ===================== MODEL ===================== */

#[derive(Serialize, Deserialize)]
enum InputMode {
    None,
    AddNote,
}

impl Default for InputMode {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Serialize, Deserialize)]
struct User {
    level: u32,
    xp: u32,
    gold: u32,
    log: Vec<String>,
    notes: Vec<String>,
    input: InputMode,
}

impl Default for User {
    fn default() -> Self {
        Self {
            level: 1,
            xp: 0,
            gold: 0,
            log: vec![],
            notes: vec![],
            input: InputMode::None,
        }
    }
}

/* ===================== STORAGE ===================== */

fn open_db() -> Db {
    sled::open("sled_db").expect("failed to open sled db")
}

fn key(user_id: u64) -> String {
    format!("user:{user_id}")
}

fn load_user(db: &Db, user_id: u64) -> User {
    db.get(key(user_id))
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or_default()
}

fn save_user(db: &Db, user_id: u64, user: &User) {
    let _ = db.insert(key(user_id), serde_json::to_vec(user).unwrap());
}

/* ===================== GAME LOGIC ===================== */

fn xp_to_next(level: u32) -> u32 {
    level * 100
}

fn log(user: &mut User, text: impl Into<String>) {
    let ts = Local::now().format("%d.%m %H:%M");
    user.log.insert(0, format!("{} — {}", ts, text.into()));
}

fn complete_quest(user: &mut User, name: &str, xp: u32, gold: u32) -> Option<u32> {
    user.xp += xp;
    user.gold += gold;

    let mut level_up = None;

    while user.xp >= xp_to_next(user.level) {
        user.xp -= xp_to_next(user.level);
        user.level += 1;
        level_up = Some(user.level);
        log(user, format!("🆙 Новый уровень {}", user.level));
    }

    log(
        user,
        format!(
            "✅ {} (+{} XP{})",
            name,
            xp,
            if gold > 0 {
                format!(", +{} золота", gold)
            } else {
                "".into()
            }
        ),
    );

    level_up
}

/* ===================== UI ===================== */

fn main_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("👤 Профиль", "profile"),
            InlineKeyboardButton::callback("📜 Квесты", "quests"),
        ],
        vec![
            InlineKeyboardButton::callback("📖 Журнал", "log"),
            InlineKeyboardButton::callback("🗒 Заметки", "notes"),
        ],
    ])
}

fn quest_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("💼 Отклик", "q_apply"),
            InlineKeyboardButton::callback("🧠 Учёба", "q_study"),
        ],
        vec![
            InlineKeyboardButton::callback("📄 Резюме", "q_resume"),
            InlineKeyboardButton::callback("✉️ Рекрутер", "q_recruiter"),
        ],
        vec![InlineKeyboardButton::callback("🛠️ Проект", "q_project")],
        vec![InlineKeyboardButton::callback("⬅️ Назад", "back")],
    ])
}

fn notes_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("➕ Добавить заметку", "add_note")],
        vec![InlineKeyboardButton::callback("⬅️ Назад", "back")],
    ])
}

/* ===================== BOT ===================== */

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init();

    let bot = Bot::from_env();
    let db = Arc::new(open_db());

    let handler = dptree::entry()
        // /start
        .branch(
            Update::filter_message()
                .filter(|m: Message| m.text() == Some("/start"))
                .endpoint({
                    let db = db.clone();
                    move |bot: Bot, msg: Message| {
                        let db = db.clone();
                        async move {
                            let Some(from) = msg.from() else { return Ok(()); };
                            let user = load_user(&db, from.id.0);
                            save_user(&db, from.id.0, &user);

                            bot.send_message(msg.chat.id, "🎮 Поиск работы — MMORPG")
                                .reply_markup(main_menu())
                                .await?;
                            Ok(())
                        }
                    }
                }),
        )
        // обычный текст (заметки)
        .branch(
            Update::filter_message()
                .filter(|m: Message| m.text().is_some())
                .endpoint({
                    let db = db.clone();
                    move |bot: Bot, msg: Message| {
                        let db = db.clone();
                        async move {
                            let Some(from) = msg.from() else { return Ok(()); };
                            let text = msg.text().unwrap();

                            let mut user = load_user(&db, from.id.0);

                            if let InputMode::AddNote = user.input {
                                user.notes.insert(0, text.to_string());
                                log(&mut user, "📝 Создана заметка");
                                user.input = InputMode::None;
                                save_user(&db, from.id.0, &user);

                                bot.send_message(msg.chat.id, "✅ Заметка сохранена")
                                    .reply_markup(main_menu())
                                    .await?;
                            }
                            Ok(())
                        }
                    }
                }),
        )
        // callback
        .branch(
            Update::filter_callback_query().endpoint({
                let db = db.clone();
                move |bot: Bot, q: CallbackQuery| {
                    let db = db.clone();
                    async move { handle_callback(bot, q, db).await }
                }
            }),
        );

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

/* ===================== CALLBACK HANDLER ===================== */

async fn handle_callback(bot: Bot, q: CallbackQuery, db: Arc<Db>) -> ResponseResult<()> {
    let Some(data) = q.data.as_deref() else {
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    };
    let Some(message) = q.message.as_ref() else {
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    };

    let user_id = q.from.id.0;
    let chat_id = message.chat().id;
    let msg_id = message.id();

    let mut user = load_user(&db, user_id);

    let (text, keyboard) = match data {
        "profile" => (
            format!(
                "👤 Уровень: {}\nXP: {} / {}\n💰 Золото: {}",
                user.level,
                user.xp,
                xp_to_next(user.level),
                user.gold
            ),
            main_menu(),
        ),
        "quests" => ("📜 Выбери квест".into(), quest_menu()),
        "log" => (
            format!("📖 Журнал\n\n{}", user.log.join("\n")),
            main_menu(),
        ),
        "notes" => (
            format!("🗒 Заметки\n\n{}", user.notes.join("\n")),
            notes_menu(),
        ),
        "add_note" => {
            user.input = InputMode::AddNote;
            ("✍️ Напиши текст заметки одним сообщением".into(), InlineKeyboardMarkup::default())
        }
        "q_apply" => quest(&mut user, "Отклик", 20, 1),
        "q_study" => quest(&mut user, "Учёба", 15, 0),
        "q_resume" => quest(&mut user, "Резюме", 30, 0),
        "q_recruiter" => quest(&mut user, "Рекрутер", 25, 1),
        "q_project" => quest(&mut user, "Проект", 50, 0),
        "back" => ("Главное меню".into(), main_menu()),
        _ => {
            bot.answer_callback_query(q.id).await?;
            return Ok(());
        }
    };

    save_user(&db, user_id, &user);

    bot.edit_message_text(chat_id, msg_id, text)
        .reply_markup(keyboard)
        .await?;

    bot.answer_callback_query(q.id).await?;
    Ok(())
}

fn quest(user: &mut User, name: &str, xp: u32, gold: u32) -> (String, InlineKeyboardMarkup) {
    let lvl = complete_quest(user, name, xp, gold);

    let mut text = format!("✅ {}\n+{} XP", name, xp);
    if gold > 0 {
        text.push_str(&format!(", +{} золота", gold));
    }
    if let Some(l) = lvl {
        text.push_str(&format!("\n🆙 Новый уровень {}", l));
    }

    (text, quest_menu())
}
