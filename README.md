# CoronaNG - Automated ASQ Registration

`corona-ng` is a Terminal User Interface (TUI) designed to help students at the **University of Ulm** secure spots in **ASQ (Additive Schlüssel Qualifikationen)** courses via the [CoronaNG portal](https://campusonline.uni-ulm.de/CoronaNG/index.html).

## The Problem
Registration for ASQ courses typically opens at a specific time. Due to the high number of students trying to register simultaneously, the university servers often become overloaded, making it nearly impossible to get through using a standard web browser. When slots are limited, every millisecond counts.

## The Solution
This application is built with **Rust** and operates **headless** (without a browser). By bypassing the overhead of a browser engine and sending HTTP requests directly to the portal, it provides a significant speed advantage over manual registration.

### Key Features
- **Headless & High Performance:** Much faster than any browser-based approach.
- **Precision Timing:** Sends multiple registration requests at the exact microsecond the registration window opens.
- **Multi-Request Strategy:** Automatically executes a burst of 10 requests (with 150ms intervals) to increase the probability of getting through during server spikes.
- **Modern TUI:** Built with `ratatui`, providing a dashboard to view observed courses.
- **Secure Credential Storage:** Uses the system keyring to securely store your university credentials.
- **Real-time Feedback:** Shows live server time vs. local time and provides detailed status reports for every registration attempt.

## Success Story
This tool was successfully used to secure a spot in a course with **180 observers** and only **20 available places**. While the website was unreachable for many, `corona-ng` successfully pushed through the registration in the first few milliseconds.

## Technical Stack
- **Language:** Rust
- **Async Runtime:** `tokio`
- **Networking:** `reqwest` (with cookie management)
- **UI:** `ratatui` & `crossterm`
- **Security:** `keyring` (OS-native credential storage)
- **HTML Parsing:** `scraper`

## Setup & Usage

### Prerequisites
- [Rust](https://rustup.rs/) installed
- Access to a kiz account

### Building from Source
```bash
cargo build --release
```

### Running
```bash
cargo run --release
```

### How to use in practise

1. **Observe courses:** Visit [the CoronaNG portal](https://campusonline.uni-ulm.de/CoronaNG/index.html) and click "observe" on your desired courses.
2. **TUI Login:** Enter your kiz credentials into the TUI.
3. **Browse:** View the list of observed courses and their current status.
4. **Schedule:** Select a course and set the registration time (e.g., `16:00`).
5. **Deploy:** Keep the app running; it will automatically trigger the registration burst at the scheduled time. A popup will open shortly before the timer ends.

> **Important:** CoronaNG allows only one session per user. Signing in to the
web-portal ends the session of the TUI, and a login by the TUI ends the session
in the browser. The TUI logs in again automatically when its session expired,
and checks its session about 60s before each scheduled target. If you sign in
on the website shortly before the target, the TUI has to log in again at the
target itself, which costs time. Do not use the website in the last minute
before a scheduled target.

## Disclaimer
This tool is for personal use and intended to assist in course registration. Please use it responsibly and in accordance with the university's IT policies.

## License
[MIT](/LICENSE), also see [licenses of dependencies](/DEPENDENCIES.md).
