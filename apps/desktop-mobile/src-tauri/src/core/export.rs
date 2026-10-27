//! Exports from the app (PRD FR-6.1–6.3, FR-6.7): the bytes and names of a file, or the QR codes to show.
//! Confirming plaintext exports and choosing where to save happen in the native layer, not here.

use keep_transfer::backup::write_backup;
use keep_transfer::export::{goldenpassport_file, google_codes, links_sheet, single_link};
use keep_transfer::goldenpassport::GpIncompatible;
use keep_transfer::migration::Incompatible;
use keep_transfer::qr::{QrEcc, qr_svg};
use keep_vault::Entry;
use keep_vault::sealed::KdfParams;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{Core, CoreError, Vault};

const MS_PER_DAY: u64 = 24 * 60 * 60 * 1000;

/// What kind of file to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    /// An encrypted OTPKeep backup.
    Backup,
    /// A plaintext list of `otpauth://` links with names and notes.
    Links,
    /// A plaintext GoldenPassport `.secrets` file.
    GoldenPassport,
}

impl ExportFormat {
    /// Whether the file holds secrets unencrypted, so the person confirms it natively first (FR-6.7).
    #[must_use]
    pub const fn is_plaintext(self) -> bool {
        !matches!(self, Self::Backup)
    }
}

/// An entry an export could not hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftOut {
    pub title: String,
    /// `STEAM`, `HOTP`, `ALGORITHM`, `DIGITS`, `PERIOD` or `MD5`.
    pub reason: &'static str,
}

/// A file ready to be saved.
pub struct ExportFile {
    pub bytes: Zeroizing<Vec<u8>>,
    pub file_name: String,
    pub extension: &'static str,
    /// How many entries the file holds.
    pub count: usize,
    pub left_out: Vec<LeftOut>,
}

/// One QR code to show, as an SVG image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrImage {
    pub svg: String,
}

/// Google Authenticator transfer codes, plus the entries to scan one by one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleExport {
    pub codes: Vec<QrImage>,
    /// Entries a transfer code cannot hold, each with its own QR code.
    pub separate: Vec<SeparateCode>,
    /// Entries using SHA-256 or SHA-512, to check after scanning.
    pub check_after: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeparateCode {
    pub title: String,
    pub reason: &'static str,
    pub svg: String,
}

impl Core {
    /// The chosen entries (all when `ids` is `None`), in list order; never the trash.
    fn chosen(&self, ids: Option<&[String]>) -> Result<Vec<&Entry>, CoreError> {
        let Vault::Open { service, .. } = &self.vault else {
            return Err(CoreError::NeedsPassword);
        };
        let all = service.ordered();
        let Some(ids) = ids else {
            return if all.is_empty() {
                Err(CoreError::Invalid("NOTHING_TO_EXPORT"))
            } else {
                Ok(all)
            };
        };
        if ids
            .iter()
            .any(|id| !all.iter().any(|e| e.id.to_string() == *id))
        {
            return Err(CoreError::NotFound);
        }
        let chosen: Vec<&Entry> = all
            .into_iter()
            .filter(|e| ids.contains(&e.id.to_string()))
            .collect();
        if chosen.is_empty() {
            return Err(CoreError::Invalid("NOTHING_TO_EXPORT"));
        }
        Ok(chosen)
    }

    /// The chosen entries, copied out so a file can be made from them without holding the core: deriving a
    /// backup's key takes a while (Argon2id at full strength), and the windows and sync keep working meanwhile.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`], for example `NOTHING_TO_EXPORT`.
    pub fn export_job(
        &self,
        format: ExportFormat,
        ids: Option<&[String]>,
    ) -> Result<ExportJob, CoreError> {
        Ok(ExportJob {
            format,
            entries: self.chosen(ids)?.into_iter().cloned().collect(),
            date: date_of((self.clock)()),
        })
    }

    /// A file of the chosen entries in one step, for tests.
    #[cfg(test)]
    pub(crate) fn export_file_with(
        &self,
        format: ExportFormat,
        ids: Option<&[String]>,
        password: Option<&str>,
        params: KdfParams,
    ) -> Result<ExportFile, CoreError> {
        self.export_job(format, ids)?.finish_with(password, params)
    }

    /// Transfer codes for Google Authenticator, to scan one after another.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when nothing is chosen or no code can be drawn.
    pub fn export_google(&self, ids: Option<&[String]>) -> Result<GoogleExport, CoreError> {
        let entries = self.chosen(ids)?;
        let export = google_codes(&entries).map_err(|_| CoreError::Save)?;
        let codes = export
            .links
            .iter()
            .map(|link| draw(link, QrEcc::Low).map(|svg| QrImage { svg }))
            .collect::<Result<_, _>>()?;
        let separate = export
            .separate
            .iter()
            .map(|(index, reason)| {
                Ok(SeparateCode {
                    title: entries[*index].title.value.clone(),
                    reason: google_reason(*reason),
                    svg: draw(&single_link(entries[*index]), QrEcc::Medium)?,
                })
            })
            .collect::<Result<_, CoreError>>()?;
        let check_after = export
            .algorithm_unverified
            .iter()
            .map(|index| entries[*index].title.value.clone())
            .collect();
        Ok(GoogleExport {
            codes,
            separate,
            check_after,
        })
    }

    /// One entry's QR code, to scan into another authenticator (PRD FR-6.3).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the entry is gone.
    pub fn entry_qr(&self, id: &str) -> Result<QrImage, CoreError> {
        let ids = [id.to_owned()];
        let entries = self.chosen(Some(&ids))?;
        let entry = entries.first().ok_or(CoreError::NotFound)?;
        Ok(QrImage {
            svg: draw(&single_link(entry), QrEcc::Medium)?,
        })
    }
}

/// The entries of an export, taken from the core; see [`Core::export_job`].
pub struct ExportJob {
    format: ExportFormat,
    entries: Vec<Entry>,
    date: String,
}

impl ExportJob {
    /// The file. A backup needs `password`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`], for example `BACKUP_PASSWORD_EMPTY`.
    pub fn finish(&self, password: Option<&str>) -> Result<ExportFile, CoreError> {
        let params = keep_vault::master::new_kdf_params().map_err(|_| CoreError::Save)?;
        self.finish_with(password, params)
    }

    /// [`ExportJob::finish`] with given key derivation parameters (quick ones in tests).
    pub(crate) fn finish_with(
        &self,
        password: Option<&str>,
        params: KdfParams,
    ) -> Result<ExportFile, CoreError> {
        let entries: Vec<&Entry> = self.entries.iter().collect();
        let date = &self.date;
        let (bytes, file_name, extension, left_out) = match self.format {
            ExportFormat::Backup => {
                let password = password
                    .filter(|p| !p.is_empty())
                    .ok_or(CoreError::Invalid("BACKUP_PASSWORD_EMPTY"))?;
                let bytes =
                    write_backup(&entries, password, params).map_err(|_| CoreError::Save)?;
                (
                    Zeroizing::new(bytes),
                    format!("otpkeep-backup-{date}.otpkeep"),
                    "otpkeep",
                    Vec::new(),
                )
            }
            ExportFormat::Links => {
                let text = links_sheet(&entries);
                (
                    Zeroizing::new(text.as_bytes().to_vec()),
                    format!("otpkeep-links-{date}.txt"),
                    "txt",
                    Vec::new(),
                )
            }
            ExportFormat::GoldenPassport => {
                let file = goldenpassport_file(&entries);
                let left_out = file
                    .skipped
                    .iter()
                    .map(|(index, reason)| LeftOut {
                        title: entries[*index].title.value.clone(),
                        reason: gp_reason(*reason),
                    })
                    .collect();
                (
                    file.bytes,
                    format!("goldenpassport-{date}.secrets"),
                    "secrets",
                    left_out,
                )
            }
        };
        Ok(ExportFile {
            bytes,
            file_name,
            extension,
            count: entries.len() - left_out.len(),
            left_out,
        })
    }
}

fn draw(text: &str, ecc: QrEcc) -> Result<String, CoreError> {
    qr_svg(text, ecc)
        .map(|svg| svg.as_str().to_owned())
        .map_err(|_| CoreError::Invalid("QR_TOO_LONG"))
}

const fn gp_reason(reason: GpIncompatible) -> &'static str {
    match reason {
        GpIncompatible::CounterBased => "HOTP",
        GpIncompatible::Algorithm => "ALGORITHM",
        GpIncompatible::Digits => "DIGITS",
        GpIncompatible::Period => "PERIOD",
        GpIncompatible::Steam => "STEAM",
    }
}

const fn google_reason(reason: Incompatible) -> &'static str {
    match reason {
        Incompatible::Period(_) => "PERIOD",
        Incompatible::Digits(_) => "DIGITS",
        Incompatible::Md5 => "MD5",
        Incompatible::Steam => "STEAM",
    }
}

/// `YYYY-MM-DD` (UTC) of Unix milliseconds, for file names.
fn date_of(now_ms: u64) -> String {
    // Days to civil date (Howard Hinnant's algorithm), valid for every date after 1970.
    let days = i64::try_from(now_ms / MS_PER_DAY).unwrap_or(0) + 719_468;
    let era = days / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests;
