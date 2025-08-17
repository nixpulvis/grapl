use grapl::{Expr, Normalize, Parse};
use microxdg::{Xdg, XdgError};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::fs;
use std::path::PathBuf;

const HISTFILE: &'static str = "grapl.history";

fn main() -> rustyline::Result<()> {
    let mut rl = DefaultEditor::new()?;

    if let Ok(mut state) = get_xdg_state_dir() {
        state.push("grapl");
        if fs::create_dir_all(&state).is_ok() {
            state.push(HISTFILE);
            rl.load_history(&state).ok();
        }
    }

    loop {
        let readline = rl.readline("> ");
        match readline {
            Ok(line) => match Expr::parse(line.as_str()).into_result() {
                Ok(expr) => {
                    rl.add_history_entry(line.as_str()).unwrap();
                    println!("{}", expr.normalize());
                }
                Err(errors) => {
                    println!("Invalid syntax:");
                    for error in errors {
                        println!("{}", error)
                    }
                }
            },
            Err(ReadlineError::Interrupted) => {
                println!("Ctrl-C pressed. Exiting.");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("Ctrl-D pressed. Exiting.");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    // TODO: Refactor to remove duplicate code in loading.
    if let Ok(mut state) = get_xdg_state_dir() {
        state.push("grapl");
        if fs::create_dir_all(&state).is_ok() {
            state.push(HISTFILE);
            rl.save_history(&state).ok();
        }
    }

    Ok(())
}

fn get_xdg_state_dir() -> Result<PathBuf, XdgError> {
    let xdg = Xdg::new()?;
    let state_dir = xdg.state()?;
    Ok(state_dir)
}
