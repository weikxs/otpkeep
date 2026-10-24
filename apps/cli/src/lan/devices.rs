//! `otpkeep devices`, `otpkeep devices remove`, `otpkeep group leave` and `otpkeep rename`
//! (design/06-sync.md §2, §7).

use keep_sync::group::{
    DeviceRow, GroupError, Staleness, device_rows, leave_group, remove_member, rename_self,
};
use serde_json::json;

use super::not_synced;
use crate::args::Options;
use crate::ask::confirm;
use crate::commands::{Printed, service_error};
use crate::open::open_service;
use crate::{CliError, Context, EXIT_INVALID_INPUT, EXIT_MULTIPLE_MATCHES, EXIT_NO_MATCH};

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// The members of this device's group.
pub(crate) fn devices(context: &mut Context<'_>, options: Options) -> Result<Printed, CliError> {
    let service = open_service(context, options)?;
    let rows = device_rows(service.doc(), (context.now_ms)()).map_err(group_error)?;
    let now = (context.now_ms)();
    let text = rows
        .iter()
        .map(|r| line(r, now))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let json = rows
        .iter()
        .map(|r| {
            json!({
                "name": r.lan_name,
                "fingerprint": r.fingerprint,
                "platform": super::platform_name(r.platform),
                "last_sync_ms": r.last_sync_ms,
                "this_device": r.is_this_device,
                "may_be_behind": r.staleness != Staleness::Fresh,
                "consider_removing": r.staleness == Staleness::ConsiderRemoving,
                "added_by_removed_device": r.added_by_removed,
            })
        })
        .collect();
    Ok(Printed {
        text,
        json: serde_json::Value::Array(json),
        notes: Vec::new(),
    })
}

fn line(row: &DeviceRow, now: u64) -> String {
    let name = row.lan_name.as_deref().unwrap_or("(unnamed)");
    let last = if row.is_this_device {
        "this device".to_owned()
    } else {
        row.last_sync_ms.map_or_else(
            || "never synced".to_owned(),
            |at| match now.saturating_sub(at) / DAY_MS {
                0 => "synced today".to_owned(),
                1 => "synced yesterday".to_owned(),
                days => format!("synced {days} days ago"),
            },
        )
    };
    let mut flags = Vec::new();
    match row.staleness {
        Staleness::MayBeBehind => flags.push("may be behind"),
        Staleness::ConsiderRemoving => flags.push("not synced for 90 days: consider removing"),
        Staleness::Fresh => {}
    }
    if row.added_by_removed {
        flags.push("added by a removed device: keep or remove it");
    }
    let flags = if flags.is_empty() {
        String::new()
    } else {
        format!("  [{}]", flags.join("; "))
    };
    format!(
        "{name}\t{}\t{}\t{last}{flags}",
        row.fingerprint,
        super::platform_name(row.platform)
    )
}

/// Removes the member whose fingerprint starts with `fingerprint`, after confirmation.
pub(crate) fn remove_device(
    context: &mut Context<'_>,
    fingerprint: &str,
    options: Options,
) -> Result<Printed, CliError> {
    let mut service = open_service(context, options)?;
    let now = (context.now_ms)();
    let rows = device_rows(service.doc(), now).map_err(group_error)?;
    let wanted = fingerprint.replace('-', "").to_uppercase();
    let matches: Vec<&DeviceRow> = rows
        .iter()
        .filter(|r| r.fingerprint.replace('-', "").starts_with(&wanted) && !wanted.is_empty())
        .collect();
    let row = match matches.as_slice() {
        [] => {
            return Err(CliError::new(
                EXIT_NO_MATCH,
                "NO_MATCH",
                "no device has that fingerprint",
            ));
        }
        [one] => *one,
        many => {
            return Err(CliError::with_candidates(
                EXIT_MULTIPLE_MATCHES,
                "MULTIPLE_MATCHES",
                "several devices match; give more of the fingerprint",
                many.iter().map(|r| r.fingerprint.clone()).collect(),
            ));
        }
    };
    let name = row.lan_name.clone().unwrap_or_default();
    confirm(
        context,
        false,
        &format!(
            "Remove {name} · {} from the sync group for good? It keeps the data it has; consider replacing important keys.",
            row.fingerprint
        ),
    )?;
    let removed = remove_member(service.doc(), row.device, now).map_err(group_error)?;
    service.commit_synced(removed, &[]).map_err(service_error)?;
    Ok(Printed {
        text: format!("Removed {name}. Other devices learn of it when they sync.\n"),
        json: json!({ "removed": row.fingerprint }),
        notes: Vec::new(),
    })
}

/// Leaves the sync group after confirmation; the entries stay.
pub(crate) fn leave(context: &mut Context<'_>, options: Options) -> Result<Printed, CliError> {
    let mut service = open_service(context, options)?;
    let me = device_rows(service.doc(), (context.now_ms)())
        .map_err(group_error)?
        .into_iter()
        .find(|r| r.is_this_device);
    confirm(
        context,
        false,
        "Leave the sync group? This device keeps its entries but stops syncing.",
    )?;
    let left = leave_group(service.doc()).map_err(group_error)?;
    service.commit_synced(left, &[]).map_err(service_error)?;
    let who = me
        .map(|r| format!("{} · {}", r.lan_name.unwrap_or_default(), r.fingerprint))
        .unwrap_or_default();
    Ok(Printed {
        text: format!("Left the sync group. Remove this device on another member: {who}\n"),
        json: json!({ "left": true, "device": who }),
        notes: Vec::new(),
    })
}

/// Renames this device in its group.
pub(crate) fn rename(
    context: &mut Context<'_>,
    name: &str,
    options: Options,
) -> Result<Printed, CliError> {
    let mut service = open_service(context, options)?;
    let renamed = rename_self(service.doc(), name, (context.now_ms)()).map_err(group_error)?;
    service.commit_synced(renamed, &[]).map_err(service_error)?;
    Ok(Printed {
        text: format!("This device is now \"{name}\". Others see it after the next sync.\n"),
        json: json!({ "name": name }),
        notes: Vec::new(),
    })
}

fn group_error(error: GroupError) -> CliError {
    match error {
        GroupError::NotInGroup => not_synced("NOT_IN_GROUP", "this device is not in a sync group"),
        GroupError::BadName => CliError::new(
            EXIT_INVALID_INPUT,
            "INVALID_INPUT",
            "the name must have 1 to 32 characters without surrounding spaces",
        ),
        GroupError::NotAMember => CliError::new(EXIT_NO_MATCH, "NO_MATCH", "no such member"),
        other => not_synced("GROUP", &other.to_string()),
    }
}
