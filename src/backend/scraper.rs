use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use scraper::{Html, Selector};

use crate::app::{BatchSignUpReport, Course, CourseSignUpResult, CourseStatus, SignUpOutcome};

pub fn parse_courses(html: &str) -> Vec<Course> {
    let document = Html::parse_document(html);
    let mut courses = Vec::new();

    let row_sel = Selector::parse("#col3_innen .coronaform.w4cform tr.dbo").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let img_sel = Selector::parse("img").unwrap();
    let checkbox_sel = Selector::parse("input[type=\"checkbox\"]").unwrap();

    for row in document.select(&row_sel) {
        let tds: Vec<_> = row.select(&td_sel).collect();

        if tds.len() < 7 {
            continue;
        }

        let id = tds[0]
            .select(&checkbox_sel)
            .next()
            .and_then(|input| input.value().attr("name"))
            .map(|name| name.replace("check_", ""))
            .unwrap_or_default();

        let name = tds[1]
            .select(&a_sel)
            .next()
            .map(|a| {
                a.text()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        let note = tds[1].text().last().unwrap_or("").trim().to_string();

        let parse_td_int = |td: &scraper::ElementRef| -> u32 {
            td.text().collect::<String>().trim().parse().unwrap_or(0)
        };

        let observations = parse_td_int(&tds[2]);
        let participants = parse_td_int(&tds[3]);
        let max_participants = parse_td_int(&tds[4]);

        let status = tds[5]
            .select(&img_sel)
            .next()
            .and_then(|img| img.value().attr("src"))
            .map(|src| match src {
                s if s.ends_with("open.gif") => CourseStatus::Open,
                s if s.ends_with("closed.gif") => CourseStatus::Closed,
                s if s.ends_with("person.gif") => CourseStatus::Enrolled,
                s if s.ends_with("max.gif") => CourseStatus::Full,
                _ => CourseStatus::Unknown,
            })
            .unwrap_or(CourseStatus::Unknown);

        courses.push(Course {
            id,
            name,
            note,
            observations,
            participants,
            max_participants,
            status,
        });
    }

    courses
}

pub fn check_login_error(html: &str) -> Option<String> {
    let document = Html::parse_document(html);

    let error_sel = Selector::parse("span.Error").unwrap();
    if let Some(error_el) = document.select(&error_sel).next() {
        let error_msg = error_el
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();

        return Some(error_msg);
    }

    let form_sel = Selector::parse("form#IndexForm").unwrap();
    if document.select(&form_sel).next().is_some() {
        return Some("Unknown Login Error".to_string());
    }

    None
}

fn parse_german_month(month: &str) -> Option<&str> {
    match month.to_lowercase().as_str() {
        "januar" => Some("01"),
        "februar" => Some("02"),
        "märz" => Some("03"),
        "april" => Some("04"),
        "mai" => Some("05"),
        "juni" => Some("06"),
        "juli" => Some("07"),
        "august" => Some("08"),
        "september" => Some("09"),
        "oktober" => Some("10"),
        "november" => Some("11"),
        "dezember" => Some("12"),
        _ => None,
    }
}

pub fn get_server_time(html: &str) -> Option<DateTime<Local>> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("#mblock_innen").unwrap();

    let full_text = document
        .select(&selector)
        .next()?
        .text()
        .collect::<String>();

    let time_part = full_text.split("Serverzeit:").nth(1)?.trim();
    let parts: Vec<&str> = time_part
        .split(|c| c == ',' || c == ' ')
        .filter(|s| !s.is_empty())
        .collect();

    if parts.len() < 5 {
        return None;
    }

    let day = parts[1].replace(".", "");
    let month_name = parts[2];
    let year = parts[3];
    let time = parts[4];

    let month = parse_german_month(month_name)?;

    let datetime_str = format!("{}-{}-{}T{}", year, month, day, time);

    NaiveDateTime::parse_from_str(&datetime_str, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .and_then(|naive| Local.from_local_datetime(&naive).single())
}

pub fn parse_sign_up_results(html: &str) -> BatchSignUpReport {
    let document = Html::parse_document(html);
    let span_selector = Selector::parse(".subcl > ul li.re > span").unwrap();

    let mut details = Vec::new();
    let mut total_success = 0;
    let mut total_failed = 0;

    let extract_first_number = |s: &str| -> u32 {
        let num_str: String = s
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        num_str.parse().unwrap_or(0)
    };

    for span in document.select(&span_selector) {
        let text = span.text().collect::<String>().trim().to_string();

        if text.contains("Teilnahme an der Teilveranstaltung") {
            let course_name = text
                .split('\'')
                .nth(1)
                .unwrap_or("Unbekannter Kurs")
                .to_string();

            let outcome = if text.contains("fehlgeschlagen") {
                SignUpOutcome::Failed(text.clone())
            } else {
                SignUpOutcome::Success
            };

            details.push(CourseSignUpResult {
                course_name,
                outcome,
            });
        } else if text.contains("Teilnahmen waren erfolgreich") {
            total_success = extract_first_number(&text);
        } else if text.contains("versuchte Teilnahmen fehlgeschlagen") {
            total_failed = extract_first_number(&text);
        }
    }

    BatchSignUpReport {
        details,
        total_success,
        total_failed,
        general_error: None,
    }
}
