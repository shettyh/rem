use std::env;

use rem_core::{Collection, SyncMode};
use rem_sync::{
    connect_collection, fetch_account_authenticated, open_system_browser, run_loopback_oauth,
    sign_out, sync_authenticated, sync_with_http, verify_dev_token, CredentialStore,
    EntitlementInfo, EntitlementState, KeyringCredentialStore, SyncReport, DEFAULT_OAUTH_TIMEOUT,
};
use serde::Serialize;

use crate::cli::{OutputFormat, SyncLoginArgs};
use crate::output::{input_error, now_millis, write_json, CliError, Success, OUTPUT_VERSION};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncStatusData {
    mode: SyncMode,
    server_url: Option<String>,
    account_id: Option<String>,
    email: Option<String>,
    provider: Option<String>,
    signed_in: bool,
    device_id: String,
    last_seq: u64,
    pending_outbox: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    entitlement: Option<EntitlementInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncRunData {
    uploaded_assets: usize,
    pushed_ops: usize,
    pulled_ops: usize,
    applied_ops: usize,
    downloaded_assets: usize,
    latest_seq: u64,
}

impl From<SyncReport> for SyncRunData {
    fn from(report: SyncReport) -> Self {
        Self {
            uploaded_assets: report.uploaded_assets,
            pushed_ops: report.pushed_ops,
            pulled_ops: report.pulled_ops,
            applied_ops: report.applied_ops,
            downloaded_assets: report.downloaded_assets,
            latest_seq: report.latest_seq,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncLoginData {
    server_url: String,
    account_id: String,
    email: Option<String>,
    provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sync: Option<SyncRunData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entitlement: Option<EntitlementInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncLogoutData {
    signed_out: bool,
}

fn format_entitlement_summary(ent: &EntitlementInfo, now_ms: i64) -> String {
    let days_left = ent.expires_at.map(|exp| {
        let remaining_ms = (exp - now_ms).max(0);
        ((remaining_ms + 86_400_000 - 1) / 86_400_000).max(1)
    });
    match ent.state {
        EntitlementState::SelfHosted => "Self-hosted".to_owned(),
        EntitlementState::Trialing => match days_left {
            Some(days) => format!("Trial ({days}d left)"),
            None => "Trial".to_owned(),
        },
        EntitlementState::Active => "Premium".to_owned(),
        EntitlementState::PastDue => "Paused (payment issue)".to_owned(),
        EntitlementState::Expired => "Paused (expired)".to_owned(),
        EntitlementState::None => "Paused (no active plan)".to_owned(),
    }
}

pub fn show_sync_status(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
    let state = collection.sync_state()?;
    let pending_outbox = collection.pending_outbox_count()?;
    let store = KeyringCredentialStore;
    let stored = match (&state.server_url, &state.account_id) {
        (Some(url), Some(account_id)) => store.load(url, account_id).ok().flatten(),
        _ => None,
    };
    let now = now_millis();
    let entitlement = if stored.is_some() {
        fetch_account_authenticated(collection, &store, now)
            .ok()
            .and_then(|profile| profile.entitlement)
    } else {
        None
    };
    let data = SyncStatusData {
        mode: state.mode,
        server_url: state.server_url,
        account_id: state.account_id,
        email: stored.as_ref().and_then(|c| c.email().map(str::to_owned)),
        provider: stored.as_ref().map(|c| c.provider().to_owned()),
        signed_in: stored.is_some()
            || env::var("REM_SYNC_TOKEN")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
        device_id: state.device_id,
        last_seq: state.last_seq,
        pending_outbox,
        entitlement,
    };
    match output {
        OutputFormat::Text => {
            println!(
                "mode\t{}",
                match data.mode {
                    SyncMode::Local => "local",
                    SyncMode::Cloud => "cloud",
                }
            );
            if let Some(url) = &data.server_url {
                println!("server\t{url}");
            }
            if let Some(account_id) = &data.account_id {
                println!("account\t{account_id}");
            }
            if let Some(email) = &data.email {
                println!("email\t{email}");
            }
            if let Some(ent) = &data.entitlement {
                println!("plan\t{}", format_entitlement_summary(ent, now));
            }
            println!("signed_in\t{}", data.signed_in);
            println!("last_seq\t{}", data.last_seq);
            println!("pending_outbox\t{}", data.pending_outbox);
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "sync.status",
            data,
        }),
    }
    Ok(())
}

pub fn run_sync(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
    let now = now_millis();
    let report = match env::var("REM_SYNC_TOKEN") {
        Ok(token) if !token.trim().is_empty() => sync_with_http(collection, token.trim(), now)?,
        _ => sync_authenticated(collection, &KeyringCredentialStore, now)?,
    };
    let data = SyncRunData::from(report);
    match output {
        OutputFormat::Text => {
            println!(
                "synced\tpushed={}\tpulled={}\tapplied={}\tuploaded_assets={}\tdownloaded_assets={}\tseq={}",
                data.pushed_ops,
                data.pulled_ops,
                data.applied_ops,
                data.uploaded_assets,
                data.downloaded_assets,
                data.latest_seq
            );
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "sync.run",
            data,
        }),
    }
    Ok(())
}

pub fn sync_login(collection: &Collection, args: SyncLoginArgs) -> Result<(), CliError> {
    let now = now_millis();
    let device_id = collection.sync_state()?.device_id;
    let credentials = if let Some(token) = args.token {
        verify_dev_token(&args.server, &token)?
    } else if let Some(provider) = args.provider {
        let no_browser = args.no_browser;
        run_loopback_oauth(
            &args.server,
            provider.into(),
            &device_id,
            now,
            DEFAULT_OAUTH_TIMEOUT,
            |url| {
                eprintln!("Open this URL to sign in to rem:\n  {url}");
                if !no_browser {
                    let _ = open_system_browser(url);
                }
                Ok(())
            },
        )?
    } else {
        return Err(input_error(
            "invalid_input",
            "provide either --provider or --token".into(),
        ));
    };

    let store = KeyringCredentialStore;
    let connected = connect_collection(
        collection,
        &store,
        &args.server,
        &credentials,
        now,
        args.reset_binding,
    )?;

    let account = fetch_account_authenticated(collection, &store, now_millis()).ok();
    let entitlement = account
        .as_ref()
        .and_then(|profile| profile.entitlement.clone());
    let can_sync = account
        .as_ref()
        .map(|profile| {
            profile
                .entitlement
                .as_ref()
                .map(|e| e.sync_enabled)
                .unwrap_or(profile.sync_enabled)
        })
        .unwrap_or(true);

    let sync_data = if can_sync {
        let report = sync_authenticated(collection, &store, now_millis())?;
        Some(SyncRunData::from(report))
    } else {
        None
    };

    match args.output {
        OutputFormat::Text => {
            if let Some(sync) = &sync_data {
                println!(
                    "connected\taccount={}\tserver={}\tseq={}",
                    connected.account_id, connected.server_url, sync.latest_seq
                );
            } else {
                println!(
                    "connected\taccount={}\tserver={}\tstatus=paused",
                    connected.account_id, connected.server_url
                );
                let premium_url = format!("{}/premium", connected.server_url.trim_end_matches('/'));
                println!(
                    "Cloud sync is paused. Open rem Desktop Settings → Cloud sync or visit {premium_url} to start your 30-day free trial or subscribe."
                );
            }
        }
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "sync.login",
            data: SyncLoginData {
                server_url: connected.server_url,
                account_id: connected.account_id,
                email: connected.email,
                provider: connected.provider,
                sync: sync_data,
                entitlement,
            },
        }),
    }
    Ok(())
}

pub fn sync_logout(collection: &Collection, output: OutputFormat) -> Result<(), CliError> {
    sign_out(collection, &KeyringCredentialStore)?;
    match output {
        OutputFormat::Text => println!("Signed out. Cloud sync paused; local cards kept."),
        OutputFormat::Json => write_json(&Success {
            version: OUTPUT_VERSION,
            command: "sync.logout",
            data: SyncLogoutData { signed_out: true },
        }),
    }
    Ok(())
}
