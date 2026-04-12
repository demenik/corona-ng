use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use scraper::{Html, Selector};
use std::collections::HashMap;

use crate::app::{
    BatchSignUpReport, Course, CourseSignUpResult, CourseStatus, SignUpOutcome, User,
};

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
            .and_then(|a| {
                a.children()
                    .find_map(|node| node.value().as_text().map(|t| t.to_string()))
            })
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
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

pub fn parse_user(html: &str) -> Option<User> {
    let document = Html::parse_document(html);

    let direct_li_selector = Selector::parse("ul.anmeldung > li").unwrap();
    let inner_li_selector = Selector::parse("ul > li").unwrap();
    let b_selector = Selector::parse("b").unwrap();

    let mut username = String::new();
    let mut first_name = String::new();
    let mut last_name = String::new();
    let mut studiengang = Vec::new();
    let mut number = String::new();

    for li in document.select(&direct_li_selector) {
        let full_text = li.text().collect::<String>().trim().to_string();

        if full_text.starts_with("Studiengang:") {
            for inner_li in li.select(&inner_li_selector) {
                let text = inner_li.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    studiengang.push(text);
                }
            }
        } else if full_text.starts_with("Matrikelnummer:") {
            let parts: Vec<&str> = full_text.split(':').collect();
            if parts.len() == 2 {
                number = parts[1].trim().to_string();
            }
        } else if full_text.contains(':') && !full_text.contains("Abmelden") {
            let parts: Vec<&str> = full_text.splitn(2, ':').collect();
            if parts.len() == 2 {
                username = parts[0].trim().to_string();

                let b_tags: Vec<_> = li.select(&b_selector).collect();
                if b_tags.len() >= 2 {
                    first_name = b_tags[0].text().collect::<String>().trim().to_string();
                    last_name = b_tags[1].text().collect::<String>().trim().to_string();
                } else {
                    let names: Vec<&str> = parts[1].trim().split_whitespace().collect();
                    if names.len() >= 2 {
                        first_name = names[0].to_string();
                        last_name = names[1..].join(" ");
                    }
                }
            }
        }
    }

    if !username.is_empty() {
        Some(User {
            username,
            first_name,
            last_name,
            studiengang,
            number,
        })
    } else {
        None
    }
}

pub fn check_login_error(html: &str) -> Result<User, String> {
    let document = Html::parse_document(html);

    let error_sel = Selector::parse("span.Error").unwrap();
    if let Some(error_el) = document.select(&error_sel).next() {
        let error_msg = error_el
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();

        return Err(error_msg);
    }

    let form_sel = Selector::parse("form#IndexForm").unwrap();
    if document.select(&form_sel).next().is_some() {
        return Err("Unknown Login Error".to_string());
    }

    Ok(parse_user(html).unwrap())
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

pub fn parse_sign_up_results(
    html: &str,
    course_map: &HashMap<String, String>,
) -> BatchSignUpReport {
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
            let raw_course_name = text.split('\'').nth(1).unwrap_or("Unbekannter Kurs");
            let course_name = raw_course_name
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");

            let outcome = if let Some(err_part) = text.split("fehlgeschlagen").nth(1) {
                let trimmed = err_part.trim();
                let cleaner = trimmed
                    .trim_start_matches(':')
                    .trim_start_matches('.')
                    .trim();
                if cleaner.is_empty() {
                    SignUpOutcome::Failed("Fehlgeschlagen".to_string())
                } else {
                    SignUpOutcome::Failed(cleaner.to_string())
                }
            } else if text.contains("fehlgeschlagen") {
                SignUpOutcome::Failed("Fehlgeschlagen".to_string())
            } else {
                SignUpOutcome::Success
            };

            let mut course_id = course_map.get(&course_name).cloned();

            if course_id.is_none() {
                // 1. Check if names are prefixes of each other
                // 2. Check if names match after stripping trailing parenthesized info (like "(Seminar)")
                fn strip_last_parens(s: &str) -> &str {
                    if let Some(pos) = s.rfind('(') {
                        s[..pos].trim()
                    } else {
                        s
                    }
                }

                let course_base = strip_last_parens(&course_name);

                for (name, id) in course_map {
                    if course_name.starts_with(name) || name.starts_with(&course_name) {
                        course_id = Some(id.clone());
                        break;
                    }

                    let map_base = strip_last_parens(name);
                    if !course_base.is_empty() && course_base == map_base {
                        course_id = Some(id.clone());
                        break;
                    }
                }
            }

            details.push(CourseSignUpResult {
                course_id,
                course_name,
                outcome,
            });
        } else if text.contains("Teilnahmen waren erfolgreich") {
            total_success = extract_first_number(&text);
        } else if text.contains("versuchte Teilnahmen fehlgeschlagen") {
            total_failed = extract_first_number(&text);
        }
    }

    let server_time = get_server_time(html).map(|dt| dt.format("%H:%M:%S").to_string());

    BatchSignUpReport {
        details,
        total_success,
        total_failed,
        general_error: None,
        server_time,
    }
}
