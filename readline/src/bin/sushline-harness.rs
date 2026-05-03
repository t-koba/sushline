use readline::{
    Config, Editor, History, HistoryExpansionContext, HistoryExpansionPolicy, Hooks, Prompt,
    ReadlineResult, Terminal, expand_history,
};
use std::process::ExitCode;
use std::{fs, path::Path};

struct HarnessApplication;

impl Hooks for HarnessApplication {
    fn expand_history(
        &mut self,
        context: HistoryExpansionContext<'_>,
    ) -> Option<Result<Vec<u8>, String>> {
        Some(
            expand_history(
                context.line,
                context.history,
                context.histchars,
                &HistoryExpansionPolicy::default(),
                |_| false,
            )
            .map_err(|err| err.message()),
        )
    }

    fn command_names(&self) -> Vec<String> {
        let mut names = vec!["echo".to_string()];
        for dir in std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        {
            let Ok(entries) = fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if is_executable_file(&path) {
                    names.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        names.sort();
        names.dedup();
        names
    }

    fn variable_names(&mut self) -> Vec<String> {
        let mut names = std::env::vars().map(|(name, _)| name).collect::<Vec<_>>();
        names.sort();
        names.dedup();
        names
    }
}

fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn main() -> ExitCode {
    let prompt = std::env::var("SUSHLINE_PROMPT").unwrap_or_default();
    let mut history = History::new();
    if let Ok(lines) = std::env::var("SUSHLINE_HISTORY") {
        for entry in lines.split('\n').filter(|entry| !entry.is_empty()) {
            history.push(entry);
        }
    }

    let mut hooks = HarnessApplication;
    let mut line = Editor::new(Config::default(), Terminal::new(), history);
    if let Ok(inputrc) = std::env::var("SUSHLINE_INPUTRC")
        && let Err(err) = line.load_inputrc_str(&inputrc)
    {
        eprintln!("SUSHLINE_INPUTRC_ERROR:{err:?}");
        return ExitCode::from(1);
    }
    if let Ok(path) = std::env::var("SUSHLINE_INPUTRC_FILE")
        && let Err(err) = line.load_inputrc_file(std::path::Path::new(&path))
    {
        eprintln!("SUSHLINE_INPUTRC_FILE_ERROR:{err:?}");
        return ExitCode::from(1);
    }

    match line.read_line(Prompt::new(prompt), &mut hooks) {
        Ok(ReadlineResult::Line(bytes)) => {
            println!("SUSHLINE_ACCEPTED:{}", String::from_utf8_lossy(&bytes));
            ExitCode::SUCCESS
        }
        Ok(ReadlineResult::Interrupted) => {
            println!("SUSHLINE_INTERRUPTED");
            ExitCode::from(130)
        }
        Ok(ReadlineResult::Eof) => {
            println!("SUSHLINE_EOF");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("SUSHLINE_ERROR:{err:?}");
            ExitCode::from(1)
        }
    }
}
