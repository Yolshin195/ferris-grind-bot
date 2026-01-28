# 🦀 ferris-grind-bot

**Ferris Grind Bot** is a Telegram bot that turns your job search into an RPG-style game.

You gain **XP, levels, gold**, complete quests, write notes, and get punished for procrastination — all to keep you consistently moving forward in your career grind.

Built with **Rust**, powered by **teloxide**, and designed to work **locally without external services**.

---

## 🎮 Concept

Job hunting is boring, stressful, and easy to procrastinate on.

`ferris-grind-bot` reframes it as a **RPG-like progression system**:

- You complete real-world actions (apply, study, improve CV)
- The bot rewards you with XP and levels
- If you do nothing — you lose XP 😈
- Every 15 minutes, the bot checks if you're still grinding

This creates **external accountability** without motivation hype.

---

## ✨ Features

### 🧙 Player Profile
- Level system with XP progression
- Gold rewards
- Persistent storage per user

### 📜 Quests
- 💼 Apply for a job
- 🧠 Study
- 📄 Improve resume
- ✉️ Contact recruiter
- 🛠️ Work on a project

Each quest gives XP and sometimes gold.

### ⏰ Procrastination Punishment
- Background reminder every 15 minutes
- If you ignore it → XP penalty
- Forced follow-up if you admit doing nothing

### 🗒 Notes System
- Store quick notes directly in chat
- Simple input mode
- Notes are persisted per user

### 📖 Activity Log
- Timestamped history of all actions
- Level-ups and penalties included

---

## 🧠 Architecture

- **Rust**
- **tokio** — async runtime
- **teloxide** — Telegram bot framework
- **sled** — embedded key-value database
- **serde** — serialization

---

## 🚀 Getting Started

### 1. Clone the repository

```bash
git clone https://github.com/yourname/ferris-grind-bot.git
cd ferris-grind-bot
```

### 2. Create `.env`

```env
TELOXIDE_TOKEN=your_telegram_bot_token
```

### 3. Run the bot

```bash
cargo run --release
```

The bot will create a local `sled_db/` directory automatically.

---

## 🕹 Usage

* `/start` — initialize your character
* Use inline buttons to navigate
* Respond to reminders honestly (or suffer XP loss)

The bot works best when left running continuously.

---

## 🔒 Privacy

* No analytics
* No external APIs
* No data leaves your machine

Everything is stored locally.

---

## 🧩 Possible Extensions

* Daily quests
* Streak system
* Boss fights (weekly goals)
* Web dashboard
* Multiple characters per user
* Export stats to JSON

---

## 🦀 Why Rust?

* Memory safety
* Low resource usage
* Excellent async model
* Perfect for long-running bots

---
