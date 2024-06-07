use druid::{
    widget::{
        prelude::{Env, Event, EventCtx},
        Controller, CrossAxisAlignment, Either, Flex, List, RawLabel, TextBox,
    },
    AppDelegate, AppLauncher, Code, Color, Command, Data, DelegateCtx, Handled, Lens, Selector,
    Target, Widget, WidgetExt, WindowDesc,
};
use std::{
    cmp::Ordering,
    env,
    ops::Deref,
    process::{self, Stdio},
    result::Result,
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
    init_env_vars().unwrap();
    let all_items = Arc::new(list_pass("").unwrap_or_default());
    let main_window = WindowDesc::new(ui_builder())
        .title("Iron Pass")
        .with_min_size((300., 600.));
    let data = AppData {
        filter: Arc::new("".into()),
        processed_filter: Arc::new("".into()),
        items: Arc::new(vec![]),
        all_items,
    };
    AppLauncher::with_window(main_window)
        .delegate(Deletate)
        .log_to_console()
        .launch(data)
        .expect("launch failed");
}

const UPDATE_LIST: Selector = Selector::new("update-list");
const COPY_PASS: Selector = Selector::new("copy-pass");
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
                let items = data
                    .all_items
                    .iter()
                    .cloned()
                    .filter(|i| i.contains(data.filter.as_ref()))
                    .enumerate()
                    .map(|(idx, value)| (value, idx == 0))
                    .map(Selected::from)
                    .collect();
                data.items = Arc::new(items);
                data.processed_filter = data.filter.clone();
            }
            Handled::Yes
        } else if cmd.is(COPY_PASS) {
            if let Some(selected) = data.items.iter().find(|s| s.is_selected()) {
                copy_pass(&selected.value).unwrap();
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

impl<W: Widget<Arc<String>>> Controller<Arc<String>, W> for TextBoxController {
    fn event(
        &mut self,
        child: &mut W,
        ctx: &mut EventCtx,
        event: &Event,
        data: &mut Arc<String>,
        env: &Env,
    ) {
        child.event(ctx, event, data, env);
        match event {
            Event::WindowConnected => ctx.set_focus(ctx.widget_id()),
            Event::KeyUp(_) => ctx.submit_command(UPDATE_LIST),
            Event::KeyDown(e) => match e.code {
                Code::KeyJ if e.mods.ctrl() => ctx.submit_command(SELECT_NEXT),
                Code::KeyK if e.mods.ctrl() => ctx.submit_command(SELECT_PREV),
                Code::Enter => ctx.submit_command(COPY_PASS),
                _ => {}
            },
            _ => {}
        }
    }
}

#[derive(Clone, Data, Lens)]
struct AppData {
    filter: Arc<String>,
    processed_filter: Arc<String>,
    items: Arc<Vec<Selected<String>>>,
    all_items: Arc<Vec<String>>,
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

fn list_pass(filter: impl AsRef<str>) -> Option<Vec<String>> {
    let cmd = process::Command::new("/opt/homebrew/bin/pass")
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
        let output = String::from_utf8_lossy(out.stdout.as_slice());
        Some(parse_list(&strip_ansi_escapes::strip_str(output)))
    }
}

fn copy_pass(filter: impl AsRef<str>) -> Result<(), String> {
    let cmd = process::Command::new("pass")
        .arg("-c")
        .arg(filter.as_ref())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let out = cmd.wait_with_output().unwrap();
    if out.status.code().unwrap() == 0 {
        Ok(())
    } else {
        Err(String::from_utf8(out.stderr).unwrap())
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

pub fn init_env_vars() -> Result<(), Error> {
    let default_shell = if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/sh"
    };
    let shell = env::var("SHELL").unwrap_or_else(|_| default_shell.into());

    let out = process::Command::new(shell)
        .arg("-ilc")
        .arg("echo -n '_SHELL_ENV_DELIMITER_'; env; echo -n '_SHELL_ENV_DELIMITER_'; exit")
        // Disables Oh My Zsh auto-update thing that can block the process.
        .env("DISABLE_AUTO_UPDATE", "true")
        // Closing stdin to prevent from programs to block on init
        .stdin(Stdio::null())
        .output()
        .map_err(Error::Shell)?;

    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let env = stdout
            .split("_SHELL_ENV_DELIMITER_")
            .nth(1)
            .ok_or_else(|| Error::InvalidOutput(stdout.clone()))?;
        for line in String::from_utf8_lossy(&strip_ansi_escapes::strip(env))
            .lines()
            .filter(|l| !l.is_empty())
        {
            let mut s = line.splitn(2, '=');
            if let Some((var, value)) = s.next().zip(s.next()) {
                env::set_var(var, value);
            }
        }
        Ok(())
    } else {
        Err(Error::EchoFailed(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Shell(#[from] std::io::Error),
    #[error("invalid output from shell echo: {0}")]
    InvalidOutput(String),
    #[error("failed to run shell echo: {0}")]
    EchoFailed(String),
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
        assert_eq!(strip_ansi_escapes::strip_str(input), "This is red text")
    }
}
