use iced::{
    widget::{column, container, text, text_input},
    Application, Command, Element, Length, Settings, Theme,
};
use regex::Regex;
use std::{
    cmp::Ordering,
    process::{self, Stdio},
};

fn main() -> iced::Result {
    HelloWorld::run(Settings::default())
}

#[derive(Debug, Clone)]
enum AppMessage {
    TextInputChanged(String),
    ListUpdate(Vec<String>),
}

#[derive(Default)]
struct HelloWorld {
    filter: String,
    items: Vec<String>,
}

impl Application for HelloWorld {
    type Executor = iced::executor::Default;
    type Message = AppMessage;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        (HelloWorld::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("Iron Pass")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            AppMessage::TextInputChanged(text) => {
                self.filter = text.clone();
                Command::perform(async move { list_pass(text) }, |list| {
                    AppMessage::ListUpdate(list.unwrap_or_default())
                })
            }
            AppMessage::ListUpdate(items) => {
                self.items = items;
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Self::Message> {
        let input = text_input("Enter key...", &self.filter).on_input(AppMessage::TextInputChanged);

        let items = self
            .items
            .iter()
            .map(|item| list_item(item))
            .collect::<Vec<_>>();

        container(column![input, column(items)])
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(20)
            .into()
    }
}

fn strip_esc_sequences(input: &str) -> String {
    let re = Regex::new(r"\x1B\[([0-9;]*[mGKH])").unwrap();
    re.replace_all(input, "").to_string()
}

fn list_item<'a>(title: impl ToString) -> Element<'a, AppMessage> {
    container(text(title)).padding(5).into()
}

fn list_pass(filter: impl AsRef<str>) -> Option<Vec<String>> {
    let cmd = process::Command::new("pass")
        .arg("find")
        .arg(filter.as_ref())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let out = cmd.wait_with_output().unwrap();
    if out.status.code().unwrap() == 1 {
        None
    } else {
        let output = String::from_utf8(out.stdout).unwrap();
        Some(parse_list(&strip_esc_sequences(&output)))
    }
}

fn parse_list(output: &str) -> Vec<String> {
    fn count_leading_whitespaces(line: &str) -> usize {
        line.chars().take_while(|c| *c == ' ').count()
    }

    fn markup_character(c: char) -> bool {
        c == '├'
            || c == ' '
            || c == '│'
            || c == '└'
            || c == 160 as char // non-breaking space
            || c == '─'
            || c == '\t'
    }

    let non_empty_lines = output.lines().filter(|l| !l.is_empty());
    let whitespaces_to_remove = non_empty_lines
        .clone()
        .map(count_leading_whitespaces)
        .min()
        .unwrap_or(0);

    let mut stack = vec![];
    let mut previous_level = 0;

    let mut result = vec![];

    for line in non_empty_lines.map(|l| &l[whitespaces_to_remove..]) {
        if line.starts_with("Search Terms:") {
            continue;
        }

        let name = line.trim_start_matches(markup_character);

        // we tracking the number of markup characters to reason about the level of inheritance
        // of pass items. Because we only comparing adjacent items on gt/lt/eq it's fine,
        // until each level of inheritance is offset by a fixed number of characters.
        let level = line.chars().count() - name.chars().count();

        match level.cmp(&previous_level) {
            Ordering::Less => {
                // Going up in tree. Removing previous item
                result.push(stack[..].join("/"));
                stack.pop();
                stack.pop();
            }
            Ordering::Equal => {
                result.push(stack[..].join("/"));
                stack.pop();
            }
            Ordering::Greater => {}
        }
        stack.push(name);
        previous_level = level;
    }
    result.push(stack[..].join("/"));

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_pass_find() {
        let items = list_pass("google");
        assert!(!items.unwrap().is_empty());
    }

    #[test]
    fn parse_output() {
        let input = "
            Search Terms: google
            ├── google.com
            │   ├── u1
            │   └── u2
            └── apple.com
                └── u1";
        let list = parse_list(input);
        assert_eq!(list, vec!["google.com/u1", "google.com/u2", "apple.com/u1"])
    }

    #[test]
    fn check_escape_sequence_remove() {
        let input = "This is \x1b[31mred\x1b[0m text";
        assert_eq!(strip_esc_sequences(input), "This is red text")
    }
}
