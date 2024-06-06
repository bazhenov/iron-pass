use druid::{
    widget::{
        prelude::{Env, Event, EventCtx},
        Controller, CrossAxisAlignment, Either, Flex, List, RawLabel, TextBox,
    },
    AppDelegate, AppLauncher, Code, Color, Command, Data, DelegateCtx, Handled, KeyEvent, Lens,
    LensExt, Selector, Target, Widget, WidgetExt, WindowDesc,
};
use regex::Regex;
use std::{
    cmp::Ordering,
    ops::Deref,
    process::{self, Stdio},
    sync::Arc,
};

#[derive(Data, Lens, Clone)]
struct Selected<T: Data> {
    value: T,
    selected: bool,
}

impl<T: Data> Selected<T> {
    fn is_selected(&self) -> bool {
        self.selected
    }

    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl<T: Data> From<T> for Selected<T> {
    fn from(value: T) -> Self {
        Selected {
            value,
            selected: false,
        }
    }
}

impl<T: Data> From<(T, bool)> for Selected<T> {
    fn from((value, selected): (T, bool)) -> Self {
        Selected { value, selected }
    }
}

impl<T: Data> Deref for Selected<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

fn main() {
    let main_window = WindowDesc::new(ui_builder()).title("Iron Pass");
    let data = AppData {
        filter: "".into(),
        processed_filter: "".into(),
        items: Arc::new(vec![]),
    };
    AppLauncher::with_window(main_window)
        .delegate(Deletate)
        .log_to_console()
        .launch(data)
        .expect("launch failed");
}

const UPDATE_LIST: Selector = Selector::new("update-list");
const SELECT_NEXT: Selector = Selector::new("select_next");
const SELECT_PREV: Selector = Selector::new("select_prev");

struct Deletate;

impl AppDelegate<AppData> for Deletate {
    fn command(
        &mut self,
        _ctx: &mut DelegateCtx,
        _target: Target,
        cmd: &Command,
        data: &mut AppData,
        _env: &Env,
    ) -> Handled {
        if cmd.is(UPDATE_LIST) {
            if data.filter != data.processed_filter {
                let items = list_pass(&data.filter)
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .map(|(idx, value)| (value, idx == 0))
                    .map(Selected::from)
                    .collect::<Vec<_>>();
                data.items = Arc::new(items);
                data.processed_filter = data.filter.clone();
            }
            Handled::Yes
        } else if cmd.is(SELECT_NEXT) {
            let selected_index = data
                .items
                .iter()
                .enumerate()
                .find_map(|(idx, v)| v.is_selected().then_some(idx))
                .unwrap_or_default();
            if selected_index < data.items.len() - 1 {
                let m = Arc::make_mut(&mut data.items);
                m[selected_index].set_selected(false);
                m[selected_index + 1].set_selected(true);
            }
            Handled::Yes
        } else if cmd.is(SELECT_PREV) {
            let selected_index = data
                .items
                .iter()
                .enumerate()
                .find_map(|(idx, v)| v.is_selected().then_some(idx))
                .unwrap_or_default();
            if selected_index > 0 {
                let m = Arc::make_mut(&mut data.items);
                m[selected_index].set_selected(false);
                m[selected_index - 1].set_selected(true);
            }
            Handled::Yes
        } else {
            Handled::No
        }
    }
}

struct TextBoxController;

impl<T, W: Widget<T>> Controller<T, W> for TextBoxController {
    fn event(&mut self, child: &mut W, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        child.event(ctx, event, data, env);
        match event {
            Event::KeyUp(_) => ctx.submit_command(UPDATE_LIST),
            Event::KeyDown(e) => match e.code {
                Code::KeyJ if e.mods.ctrl() => ctx.submit_command(SELECT_NEXT),
                Code::KeyK if e.mods.ctrl() => ctx.submit_command(SELECT_PREV),
                _ => {}
            },
            _ => {}
        }
    }
}

#[derive(Clone, Data, Lens)]
struct AppData {
    filter: String,
    processed_filter: String,
    items: Arc<Vec<Selected<String>>>,
}

fn ui_builder() -> impl Widget<AppData> {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .with_child(
            TextBox::new()
                .controller(TextBoxController)
                .lens(AppData::filter)
                .expand_width()
                .padding(10.0),
        )
        .with_flex_child(
            List::new(make_pass_item)
                .lens(AppData::items)
                .padding(10.0)
                .scroll()
                .vertical(),
            1.,
        )
}

fn make_pass_item() -> impl Widget<Selected<String>> {
    Either::new(
        |data: &Selected<String>, _env| data.is_selected(),
        RawLabel::new()
            .with_text_color(Color::rgb(1., 0., 1.))
            .lens(Selected::value),
        RawLabel::new().lens(Selected::value),
    )
}

fn strip_esc_sequences(input: &str) -> String {
    let re = Regex::new(r"\x1B\[([0-9;]*[mGKH])").unwrap();
    re.replace_all(input, "").to_string()
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
