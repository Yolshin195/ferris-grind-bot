use chrono::{Local, Utc};
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::{sync::Arc, time::Duration};
use teloxide::{
    dispatching::{Dispatcher, UpdateFilterExt},
    dptree,
    prelude::*,
    types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup},
};

/* =========================================================
   DOMAIN (Entities + pure logic)
   ========================================================= */

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

#[derive(Serialize, Deserialize, Default)]
struct User {
    level: u32,
    xp: u32,
    gold: u32,
    log: Vec<String>,
    notes: Vec<String>,
    input: InputMode,

    awaiting_ping: bool,
    last_ping_ts: i64,
}

fn xp_to_next(level: u32) -> u32 {
    level * 100
}

/* =========================================================
   REPOSITORY (DB access)
   ========================================================= */

struct UserRepository {
    db: Db,
}

impl UserRepository {
    fn new() -> Self {
        Self {
            db: sled::open("sled_db").expect("failed to open sled db"),
        }
    }

    fn key(user_id: u64) -> String {
        format!("user:{user_id}")
    }

    fn load(&self, user_id: u64) -> User {
        self.db
            .get(Self::key(user_id))
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default()
    }

    fn save(&self, user_id: u64, user: &User) {
        let _ = self
            .db
            .insert(Self::key(user_id), serde_json::to_vec(user).unwrap());
    }

    fn all(&self) -> Vec<(u64, User)> {
        self.db
            .scan_prefix("user:")
            .filter_map(|res| {
                let (k, v) = res.ok()?;
                let id = String::from_utf8_lossy(&k)
                    .replace("user:", "")
                    .parse()
                    .ok()?;
                let user = serde_json::from_slice(&v).ok()?;
                Some((id, user))
            })
            .collect()
    }
}

/* =========================================================
   SERVICE (Business logic)
   ========================================================= */

struct UserService {
    repo: Arc<UserRepository>,
}

impl UserService {
    fn new(repo: Arc<UserRepository>) -> Self {
        Self { repo }
    }

    fn load(&self, user_id: u64) -> User {
        self.repo.load(user_id)
    }

    fn save(&self, user_id: u64, user: &User) {
        self.repo.save(user_id, user)
    }

    fn log(user: &mut User, text: impl Into<String>) {
        let ts = Local::now().format("%d.%m %H:%M");
        user.log.insert(0, format!("{} — {}", ts, text.into()));
    }

    fn punish(user: &mut User, xp: u32) {
        user.xp = user.xp.saturating_sub(xp);
        Self::log(user, format!("❌ Прокрастинация (-{} XP)", xp));
    }

    fn complete_quest(
        user: &mut User,
        name: &str,
        xp: u32,
        gold: u32,
    ) -> Option<u32> {
        user.xp += xp;
        user.gold += gold;

        let mut level_up = None;
        while user.xp >= xp_to_next(user.level) {
            user.xp -= xp_to_next(user.level);
            user.level += 1;
            level_up = Some(user.level);
            Self::log(user, format!("🆙 Новый уровень {}", user.level));
        }

        Self::log(
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
}

/* =========================================================
   UI (keyboards)
   ========================================================= */

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

fn reminder_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("✅ Делаю", "doing"),
        InlineKeyboardButton::callback("❌ Ничего", "nothing"),
    ]])
}

fn force_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("✅ Сделал", "forced_done"),
    ]])
}

/* =========================================================
   BOT
   ========================================================= */

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init();

    let bot = Bot::from_env();
    let repo = Arc::new(UserRepository::new());
    let service = Arc::new(UserService::new(repo.clone()));

    /* ===== BACKGROUND REMINDER ===== */
    {
        let bot = bot.clone();
        let service = service.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(900));
            loop {
                interval.tick().await;

                for (user_id, mut user) in service.repo.all() {
                    if user.awaiting_ping {
                        UserService::punish(&mut user, 20);
                    }

                    user.awaiting_ping = true;
                    user.last_ping_ts = Utc::now().timestamp();
                    service.save(user_id, &user);

                    let _ = bot
                        .send_message(
                            ChatId(user_id as i64),
                            "⏰ Что ты сделал для поиска работы?",
                        )
                        .reply_markup(reminder_menu())
                        .await;
                }
            }
        });
    }

    let handler = dptree::entry()
        // /start
        .branch(
            Update::filter_message()
                .filter(|m: Message| m.text() == Some("/start"))
                .endpoint({
                    let service = service.clone();
                    move |bot: Bot, msg: Message| {
                        let service = service.clone();
                        async move {
                            let Some(from) = msg.from() else { return Ok(()); };
                            let user = service.load(from.id.0);
                            service.save(from.id.0, &user);

                            bot.send_message(msg.chat.id, "🎮 Поиск работы — MMORPG")
                                .reply_markup(main_menu())
                                .await?;
                            Ok(())
                        }
                    }
                }),
        )
        // text (notes)
        .branch(
            Update::filter_message()
                .filter(|m: Message| m.text().is_some())
                .endpoint({
                    let service = service.clone();
                    move |bot: Bot, msg: Message| {
                        let service = service.clone();
                        async move {
                            let Some(from) = msg.from() else { return Ok(()); };
                            let text = msg.text().unwrap();
                            let mut user = service.load(from.id.0);

                            if let InputMode::AddNote = user.input {
                                user.notes.insert(0, text.to_string());
                                UserService::log(&mut user, "📝 Создана заметка");
                                user.input = InputMode::None;
                                service.save(from.id.0, &user);

                                bot.send_message(msg.chat.id, "✅ Заметка сохранена")
                                    .reply_markup(main_menu())
                                    .await?;
                            }
                            Ok(())
                        }
                    }
                }),
        )
        // callbacks
        .branch(
            Update::filter_callback_query().endpoint({
                let service = service.clone();
                move |bot: Bot, q: CallbackQuery| {
                    let service = service.clone();
                    async move { handle_callback(bot, q, service).await }
                }
            }),
        );

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

/* =========================================================
   CALLBACKS
   ========================================================= */

async fn handle_callback(
    bot: Bot,
    q: CallbackQuery,
    service: Arc<UserService>,
) -> ResponseResult<()> {
    let Some(data) = q.data.as_deref() else { return Ok(()) };
    let Some(msg) = q.message.as_ref() else { return Ok(()) };

    let user_id = q.from.id.0;
    let chat_id = msg.chat().id;
    let msg_id = msg.id();

    let mut user = service.load(user_id);

    let (text, kb) = match data {
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
        "log" => (format!("📖 Журнал\n\n{}", user.log.join("\n")), main_menu()),
        "notes" => (format!("🗒 Заметки\n\n{}", user.notes.join("\n")), notes_menu()),
        "add_note" => {
            user.input = InputMode::AddNote;
            ("✍️ Напиши текст заметки".into(), InlineKeyboardMarkup::default())
        }
        "doing" => {
            user.awaiting_ping = false;
            ("👍 Отлично, продолжай".into(), main_menu())
        }
        "nothing" => {
            user.awaiting_ping = false;
            let bot = bot.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let _ = bot
                    .send_message(chat_id, "⏳ Ты сделал хотя бы один отклик?")
                    .reply_markup(force_menu())
                    .await;
            });
            ("⚠️ Сделай один отклик прямо сейчас".into(), InlineKeyboardMarkup::default())
        }
        "forced_done" => {
            UserService::complete_quest(&mut user, "Отклик", 20, 1);
            ("✅ Засчитано".into(), main_menu())
        }
        "q_apply" => quest(&mut user, "Отклик", 20, 1),
        "q_study" => quest(&mut user, "Учёба", 15, 0),
        "q_resume" => quest(&mut user, "Резюме", 30, 0),
        "q_recruiter" => quest(&mut user, "Рекрутер", 25, 1),
        "q_project" => quest(&mut user, "Проект", 50, 0),
        "back" => ("Главное меню".into(), main_menu()),
        _ => return Ok(()),
    };

    service.save(user_id, &user);

    bot.edit_message_text(chat_id, msg_id, text)
        .reply_markup(kb)
        .await?;

    bot.answer_callback_query(q.id).await?;
    Ok(())
}

fn quest(user: &mut User, name: &str, xp: u32, gold: u32) -> (String, InlineKeyboardMarkup) {
    let lvl = UserService::complete_quest(user, name, xp, gold);

    let mut text = format!("✅ {}\n+{} XP", name, xp);
    if gold > 0 {
        text.push_str(&format!(", +{} золота", gold));
    }
    if let Some(l) = lvl {
        text.push_str(&format!("\n🆙 Новый уровень {}", l));
    }

    (text, quest_menu())
}
