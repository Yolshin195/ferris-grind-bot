use chrono::Local;
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::sync::Arc;
use teloxide::{prelude::*, utils::command::BotCommands};

/* ===================== MODEL ===================== */

#[derive(Serialize, Deserialize, Default)]
struct User {
    level: u32,
    xp: u32,
    gold: u32,
    log: Vec<String>,
    notes: Vec<String>,
}

/* ===================== STORAGE ===================== */

fn open_db() -> Db {
    sled::open("sled_db").expect("failed to open sled db")
}

fn user_key(user_id: u64) -> String {
    format!("user:{}", user_id)
}

fn load_user(db: &Db, user_id: u64) -> User {
    db.get(user_key(user_id))
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or_else(|| User {
            level: 1,
            xp: 0,
            gold: 0,
            log: vec![],
            notes: vec![],
        })
}

fn save_user(db: &Db, user_id: u64, user: &User) {
    let bytes = serde_json::to_vec(user).unwrap();
    db.insert(user_key(user_id), bytes).unwrap();
    db.flush().ok();
}

/* ===================== GAME LOGIC ===================== */

fn xp_to_next(level: u32) -> u32 {
    level * 100
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
        user.log
            .insert(0, format!("🆙 Новый уровень: {}", user.level));
    }

    user.log.insert(
        0,
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

/* ===================== COMMANDS ===================== */

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "🎮 Поиск работы — MMORPG")]
enum Command {
    #[command(description = "Запуск бота")]
    Start,
    #[command(description = "Профиль персонажа")]
    Profile,
    #[command(description = "Список квестов")]
    Quest,
    #[command(description = "Журнал действий")]
    Log,
    #[command(description = "Добавить заметку")]
    Note(String),
    #[command(description = "Показать заметки")]
    Notes,
    #[command(description = "Отклик на вакансию")]
    Apply,
    #[command(description = "Учёба")]
    Study,
    #[command(description = "Обновить резюме")]
    Resume,
    #[command(description = "Написать рекрутеру")]
    Recruiter,
    #[command(description = "Сделать проект")]
    Project,
}

/* ===================== BOT ===================== */

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init();

    let bot = Bot::from_env();
    let db = Arc::new(open_db());

    Command::repl(bot, move |bot, msg, cmd| {
        let db = db.clone();
        async move { handle_command(bot, msg, cmd, db).await }
    })
        .await;
}

async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    db: Arc<Db>,
) -> ResponseResult<()> {
    let user_id = msg.from().unwrap().id.0;
    let mut user = load_user(&db, user_id);

    match cmd {
        Command::Start => {
            bot.send_message(
                msg.chat.id,
                "🎮 *Поиск работы — MMORPG*\n\n\
Каждое действие = XP\n\n\
/profile — персонаж\n\
/quest — квесты\n\
/log — журнал\n\
/note текст — заметка\n\
/notes — заметки",
            )
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await?;
        }

        Command::Profile => {
            bot.send_message(
                msg.chat.id,
                format!(
                    "👤 *Персонаж*\n\nУровень: {}\nXP: {} / {}\nЗолото: {}",
                    user.level,
                    user.xp,
                    xp_to_next(user.level),
                    user.gold
                ),
            )
                .parse_mode(teloxide::types::ParseMode::Markdown)
                .await?;
        }

        Command::Quest => {
            bot.send_message(
                msg.chat.id,
                "📜 *Квесты*\n\n\
/apply — 💼 Отклик (+20 XP, +1 золото)\n\
/study — 🧠 Учёба (+15 XP)\n\
/resume — 📄 Резюме (+30 XP)\n\
/recruiter — ✉️ Рекрутер (+25 XP, +1 золото)\n\
/project — 🛠️ Проект (+50 XP)",
            )
                .parse_mode(teloxide::types::ParseMode::Markdown)
                .await?;
        }

        Command::Log => {
            let text = user
                .log
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");

            bot.send_message(
                msg.chat.id,
                format!(
                    "📖 *Журнал*\n\n{}",
                    if text.is_empty() { "Пусто" } else { &text }
                ),
            )
                .parse_mode(teloxide::types::ParseMode::Markdown)
                .await?;
        }

        Command::Note(text) => {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M").to_string();

            let note = format!("{} — {}", timestamp, text);
            user.notes.insert(0, note);

            // 🔹 ВАЖНО: фиксируем факт создания заметки в журнале
            user.log
                .insert(0, format!("📝 Создана заметка ({})", timestamp));

            bot.send_message(msg.chat.id, "📝 Заметка сохранена").await?;
        }

        Command::Notes => {
            let text = user
                .notes
                .iter()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");

            bot.send_message(
                msg.chat.id,
                format!(
                    "🗒 *Заметки*\n\n{}",
                    if text.is_empty() {
                        "Нет заметок"
                    } else {
                        &text
                    }
                ),
            )
                .parse_mode(teloxide::types::ParseMode::Markdown)
                .await?;
        }

        Command::Apply => quest(&bot, &msg, &mut user, "Отклик на вакансию", 20, 1).await?,
        Command::Study => quest(&bot, &msg, &mut user, "Изучал Rust / AI", 15, 0).await?,
        Command::Resume => quest(&bot, &msg, &mut user, "Обновил резюме", 30, 0).await?,
        Command::Recruiter => {
            quest(&bot, &msg, &mut user, "Написал рекрутеру", 25, 1).await?
        }
        Command::Project => quest(&bot, &msg, &mut user, "Сделал проект", 50, 0).await?,
    }

    save_user(&db, user_id, &user);
    Ok(())
}

async fn quest(
    bot: &Bot,
    msg: &Message,
    user: &mut User,
    name: &str,
    xp: u32,
    gold: u32,
) -> ResponseResult<()> {
    let level_up = complete_quest(user, name, xp, gold);

    let mut text = format!("✅ {}\n+{} XP", name, xp);
    if gold > 0 {
        text.push_str(&format!(", +{} золота", gold));
    }
    if let Some(level) = level_up {
        text.push_str(&format!("\n🆙 Новый уровень: {}", level));
    }

    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    fn empty_user() -> User {
        User {
            level: 1,
            xp: 0,
            gold: 0,
            log: vec![],
            notes: vec![],
        }
    }

    /* ===================== XP ===================== */

    #[test]
    fn xp_to_next_is_linear() {
        assert_eq!(xp_to_next(1), 100);
        assert_eq!(xp_to_next(2), 200);
        assert_eq!(xp_to_next(5), 500);
    }

    /* ===================== QUEST ===================== */

    #[test]
    fn quest_adds_xp_and_gold() {
        let mut user = empty_user();

        let level_up = complete_quest(&mut user, "Test quest", 20, 3);

        assert_eq!(user.xp, 20);
        assert_eq!(user.gold, 3);
        assert_eq!(user.level, 1);
        assert!(level_up.is_none());
    }

    #[test]
    fn quest_can_level_up() {
        let mut user = empty_user();

        let level_up = complete_quest(&mut user, "Big quest", 150, 0);

        assert_eq!(user.level, 2);
        assert_eq!(user.xp, 50); // 150 - 100
        assert_eq!(level_up, Some(2));
    }

    #[test]
    fn quest_writes_to_log() {
        let mut user = empty_user();

        complete_quest(&mut user, "Logged quest", 10, 0);

        assert!(!user.log.is_empty());
        assert!(user.log[0].contains("Logged quest"));
    }

    #[test]
    fn level_up_is_logged() {
        let mut user = empty_user();

        complete_quest(&mut user, "Level quest", 200, 0);

        let joined = user.log.join("\n");
        assert!(joined.contains("Новый уровень"));
    }

    /* ===================== NOTES ===================== */

    #[test]
    fn note_is_saved() {
        let mut user = empty_user();

        let text = "Test note";
        let note = format!("2026-01-01 00:00 — {}", text);
        user.notes.insert(0, note);

        assert_eq!(user.notes.len(), 1);
        assert!(user.notes[0].contains(text));
    }

    #[test]
    fn note_creation_is_logged() {
        let mut user = empty_user();

        user.log.insert(0, "📝 Создана заметка".to_string());

        assert!(!user.log.is_empty());
        assert!(user.log[0].contains("заметка"));
    }
}
