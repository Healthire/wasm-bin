use std::io;
use std::io::Write;
use std::fs::File;
use std::process::Command;
use std::path::{Path, PathBuf};

use util;
use util::prompt_confirm;

const YARN_MISSING: &str =
    "No installation of yarn found. yarn is required to install webpack. https://yarnpkg.com/";
const WEBPACK_INSTALL_PROMPT: &str =
    "No installation of webpack found. Do you want to install webpack? (y/n)";

#[cfg(target_os = "windows")]
const YARN_CMD: &str = "yarn.cmd";
#[cfg(not(target_os = "windows"))]
const YARN_CMD: &str = "yarn";

#[cfg(target_os = "windows")]
const WEBPACK_CMD: &str = "webpack.cmd";
#[cfg(not(target_os = "windows"))]
const WEBPACK_CMD: &str = "webpack";

// To run batch files on windows they must be run through cmd
// https://github.com/rust-lang/rust/issues/42791
fn yarn_command() -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.arg("/k").arg("yarn.cmd");
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    Command::new("yarn")
}
fn webpack_command() -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.arg("/k").arg("webpack.cmd");
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    Command::new("webpack")
}

#[derive(Debug)]
pub enum Error {
    YarnMissing,
    YarnCommandError(io::Error),
    PromptError(util::Error),
    WebpackMissing,
    WebpackCommandError(io::Error),
    InstallFailed,
    InstallCommandError(io::Error),
    PackageFailed,
    PackageCommandError(io::Error),
    WriteJsIndexError(io::Error),
    WriteHtmlIndexError(io::Error),
}

pub fn install_if_required(skip_prompt: bool) -> Result<(), Error> {
    // This will not actually correctly run yarn on windows because running batch
    // files as processes is not correctly supported. It will however still return
    // the NotFound error if the command is not found, which is the only thing
    // we want out of running this command.
    // This is really hacky, figure out a better way of checking for installed batch scripts.
    let yarn = Command::new(YARN_CMD).arg("-v").output();
    match yarn {
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                panic!(YARN_MISSING);
            }
            _ => return Err(Error::YarnCommandError(e)),
        },
        _ => {}
    }

    // Check if webpack is installed, and if not, prompt the user to install it
    // This will not correctly run `webpack -v`, but will produce the NotFound error
    // if webpack is not found.
    match Command::new(WEBPACK_CMD).arg("-v").output() {
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                if skip_prompt
                    || prompt_confirm(WEBPACK_INSTALL_PROMPT).map_err(Error::PromptError)?
                {
                    install().unwrap();
                    Ok(())
                } else {
                    Err(Error::WebpackCommandError(e))
                }
            }
            _ => Err(Error::WebpackCommandError(e)),
        },
        _ => Ok(()),
    }
}

fn install() -> Result<(), Error> {
    match yarn_command()
        .arg("global")
        .arg("add")
        .arg("webpack")
        .arg("webpack-cli")
        .output()
    {
        Ok(output) => match output.status.success() {
            true => Ok(()),
            false => {
                println!("{}", String::from_utf8_lossy(&output.stderr));
                Err(Error::InstallFailed)
            }
        },
        Err(e) => Err(Error::InstallCommandError(e)),
    }
}

fn create_js_index(target_name: &str, dir: &Path) -> Result<(), Error> {
    let content = format!(
        r#"
        void async function () {{
            const js = await import("./{}");
            js.web_main()
        }}();
        "#,
        target_name
    );
    let js_path: PathBuf = [dir, Path::new("index.js")].iter().collect();
    let mut js_index = File::create(&js_path).map_err(Error::WriteHtmlIndexError)?;
    js_index
        .write_all(content.as_bytes())
        .map_err(Error::WriteHtmlIndexError)?;
    js_index.flush().map_err(Error::WriteHtmlIndexError)?;

    Ok(())
}

fn create_html_index(target_name: &str, dir: &Path) -> Result<(), Error> {
    let content = format!(
        r#"
        <html>
            <head>
                <meta content="text/html;charset=utf-8" http-equiv="Content-Type"/>
            </head>
            <body>
                <script src='./{}.js'></script>
            </body>
        </html>"#,
        target_name
    );
    let html_path: PathBuf = [dir, Path::new(&format!("{}.html", target_name))]
        .iter()
        .collect();
    let mut html_index = File::create(&html_path).map_err(Error::WriteHtmlIndexError)?;
    html_index
        .write_all(content.as_bytes())
        .map_err(Error::WriteHtmlIndexError)?;
    html_index.flush().map_err(Error::WriteHtmlIndexError)?;

    Ok(())
}

pub fn package_bin(target_name: &str, path: &Path) -> Result<PathBuf, Error> {
    let build_dir = path.with_file_name("");
    create_js_index(target_name, &build_dir)?;

    let out_dir = build_dir.parent().unwrap().to_path_buf();
    let out_file: PathBuf = [&out_dir, Path::new(&format!("{}.js", target_name))]
        .iter()
        .collect();
    // Package the js index file into a bundle
    match webpack_command()
        .arg(path.with_file_name("index.js"))
        .arg("--output")
        .arg(&out_file)
        .arg("--mode")
        .arg("development")
        .output()
    {
        Ok(output) => match output.status.success() {
            true => {
                println!("{}", String::from_utf8_lossy(&output.stdout));
            }
            false => return Err(Error::PackageFailed),
        },
        Err(e) => return Err(Error::PackageCommandError(e)),
    }

    create_html_index(target_name, &out_dir)?;

    Ok(out_file)
}
