use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::process::ExitCode;
use argh::FromArgs;
use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{FetchOptions, Repository};
use reqwest::{Client, StatusCode, Url};
use serde_derive::Deserialize;
use futures_util::{stream, StreamExt};

#[derive(FromArgs)]
#[argh(description = "Tool for creating backups from Github organisations")]
struct GhBackup {
    #[argh(switch, short = 'd')]
    #[argh(description = "perform a dry run. No data will be persisted.")]
    dry: bool,

    #[argh(option)]
    #[argh(description = "optional path to the backup directory. Defaults to: ./organisation-backup")]
    backup_dir: Option<PathBuf>,

    #[argh(positional)]
    #[argh(description = "name of the github organisation.")]
    organisation: String,
}

#[derive(Deserialize)]
struct GhRepo {
    name: String,
    full_name: String,
    clone_url: String,
}

#[derive(Deserialize)]
struct GhUser {
    login: String,
}

enum FetchReposError {
    OrganisationNotFound,
    Forbidden,
    ServerError,
    UnknownError,
}

impl Debug for FetchReposError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchReposError::OrganisationNotFound => write!(f, "Organisation not found."),
            FetchReposError::Forbidden => write!(f, "Access forbidden."),
            FetchReposError::ServerError => write!(f, "Server error."),
            FetchReposError::UnknownError => write!(f, "Unknown error.")
        }
    }
}

impl Display for FetchReposError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}


enum UserError {
    Forbidden,
    ServerError,
    UnknownError,
}

impl Debug for UserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UserError::Forbidden => write!(f, "Access forbidden."),
            UserError::ServerError => write!(f, "Server error."),
            UserError::UnknownError => write!(f, "Unknown error.")
        }
    }
}

impl Display for UserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for UserError {}

const MAX_PAGE: usize = 1000;

async fn fetch_user(gh_token: &str) -> Result<GhUser, UserError> {
    let client = Client::new();
    let url = Url::parse(
        "https://api.github.com/user",
    ).map_err(|e| UserError::UnknownError)?;

    let response = client.get(url.as_str())
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent",  "request")
        .bearer_auth(&gh_token)
        .send().await.map_err(|e| UserError::UnknownError)?;

    let code = response.status();
    if code == StatusCode::FORBIDDEN {
        return Err(UserError::Forbidden);
    }

    if code.is_server_error() {
        return Err(UserError::ServerError);
    }

    if !code.is_success() {
        return Err(UserError::UnknownError);
    }

    let user: GhUser = response
        .json().await
        .map_err(|e| UserError::UnknownError)?;

    Ok(user)
}
async fn fetch_repos(organisation: &str, gh_token:  &str) -> Result<Vec<GhRepo>, FetchReposError> {
    let mut repos = vec![];

    for page in 1..MAX_PAGE {
        let client = Client::new();
        let url = Url::parse_with_params(
            format!("https://api.github.com/orgs/{}/repos", organisation).as_str(),
            &[("page", page.to_string().as_str()), ("type", "all")],
        ).map_err(|e| FetchReposError::UnknownError)?;

        let response = client.get(url.as_str())
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent",  "request")
            .bearer_auth(&gh_token)
            .send().await.map_err(|e| FetchReposError::UnknownError)?;

        let code = response.status();

        if code == StatusCode::NOT_FOUND {
            return Err(FetchReposError::OrganisationNotFound);
        }

        if code == StatusCode::FORBIDDEN {
            return Err(FetchReposError::Forbidden);
        }

        if code.is_server_error() {
            return Err(FetchReposError::ServerError);
        }

        if !code.is_success() {
            return Err(FetchReposError::UnknownError);
        }

        let mut response_repos: Vec<GhRepo> = response
            .json().await
            .map_err(|e| FetchReposError::UnknownError)?;

        if response_repos.is_empty() {
            break;
        }

        repos.append(&mut response_repos);
    }

    Ok(repos)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let cli: GhBackup = argh::from_env();

    let organisation = cli.organisation;
    let backup_dir = cli.backup_dir.unwrap_or(format!("{}_backup", organisation).into());

    if backup_dir.exists() {
        eprintln!("Backup directory {} does already exist", backup_dir.display());
    }

    let Ok(gh_token) = std::env::var("GH_TOKEN").or(std::env::var("GITHUB_TOKEN")) else {
        eprintln!("Set the Github token via the environment variables GH_TOKEN or GITHUB_TOKEN.");
        return ExitCode::FAILURE;
    };

    println!("Getting user info");
    let user = match fetch_user(&gh_token).await {
        Ok(user) => user,
        Err(e) => {
            eprintln!("Failed to fetch repos: {}", e);
            return ExitCode::FAILURE;
        }
    };

    println!("Getting repos");
    let repos = match fetch_repos(&organisation, &gh_token).await {
        Ok(repos) => repos,
        Err(e) => {
            eprintln!("Failed to fetch repos: {}", e);
            return ExitCode::FAILURE;
        }
    };

    if let Err(e) = fs::create_dir_all(&backup_dir) {
        eprintln!("Failed to create backup directory: {}", e);
        return ExitCode::FAILURE;
    };

    let handles: Vec<_> = repos
        .into_iter()
        .map(|repo| {
            let backup_dir = backup_dir.clone();
            let username = user.login.clone();
            let gh_token = gh_token.clone();
            tokio::task::spawn(
            async move {
                println!("Started to backup: {} from {}", repo.full_name, repo.clone_url);

                let mut builder = CheckoutBuilder::new();
                builder.dry_run();

                let mut cb = git2::RemoteCallbacks::new();
                cb.credentials(|a, b, c| git2::Cred::userpass_plaintext(&username, &gh_token));

                let mut fo = FetchOptions::new();
                fo.remote_callbacks(cb)
                    .download_tags(git2::AutotagOption::All)
                    .update_fetchhead(true);

                let repo_dir = backup_dir.join(repo.name);
                if repo_dir.exists() {
                    let repo = Repository::open(repo_dir).unwrap();
                    for remote_name in repo.remotes().unwrap().iter() {

                        repo.find_remote(remote_name.unwrap()).unwrap().download(&[] as &[&str], Some(&mut fo)).unwrap();
                    };
                } else {
                    match RepoBuilder::new()
                        .fetch_options(fo)
                        .with_checkout(builder)
                        .clone(&repo.clone_url, repo_dir.as_path()) {
                        Ok(_repo) => ExitCode::SUCCESS,
                        Err(e) => {
                            eprintln!("Failed to clone: {}", e);
                            ExitCode::FAILURE
                        }
                    };
                }
            })
        })
        .collect();

    stream::iter(handles)
        .buffer_unordered(10)
        .collect::<Vec<_>>().await;

    ExitCode::SUCCESS
}
