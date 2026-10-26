import { toErrorMessage } from "@/lib/errorMessage";

export function webDavErrorDetail(t: (key: string) => string, error: unknown): string {
  const message = toErrorMessage(error);
  if (/WebDAV backup download (?:interrupted|incomplete)/i.test(message)) return t("settings.backupErrorDownloadIncomplete");
  if (/backup.*(?:expanded size|file count).*limit/i.test(message)) return t("settings.backupErrorExpandedTooLarge");
  if (/(?:JSON backup|Backup manifest|Backup skill manifest).*size limit/i.test(message)) return t("settings.backupErrorManifestTooLarge");
  if (/backup.*(?:corrupt|incomplete|not a valid|not valid|missing manifest)/i.test(message)) return t("settings.backupErrorCorrupt");
  if (/backup.*(?:duplicate|conflicting|symbolic links|unsafe|invalid.*path)|invalid base64|Invalid backup entry path/i.test(message)) return t("settings.backupErrorUnsafe");
  if (/backup.*(?:stage|staging|download file|finish backup download)|Failed to (?:stage backup|copy restored)|disk full|no space left|os error 112/i.test(message)) return t("settings.backupErrorStaging");
  if (/backup restore worker failed/i.test(message)) return t("settings.backupErrorWorker");
  if (/WebDAV download[^\n]*timed out/i.test(message)) {
    return t("settings.webdavErrorDownloadTimeout");
  }
  if (/timed out/i.test(message)) {
    return t("settings.webdavErrorTimeout");
  }
  if (/connection failed|error sending request|connection reset|connection closed/i.test(message)) {
    return t("settings.webdavErrorConnection");
  }
  if (/status (401|403)\b/i.test(message)) {
    return t("settings.webdavErrorUnauthorized");
  }
  if (/status [45]\d\d\b/i.test(message)) {
    return t("settings.webdavErrorRemote");
  }
  if (/Invalid WebDAV XML|invalid percent encoding|unsafe filename/i.test(message)) {
    return t("settings.webdavErrorResponse");
  }
  if (
    /URL cannot be empty|URL is invalid|URL must use|URL must not include|remote path|path segments/i.test(
      message
    )
  ) {
    return t("settings.webdavErrorConfig");
  }
  if (/size limit/i.test(message)) {
    return t("settings.webdavErrorTooLarge");
  }
  if (/backup export failed/i.test(message)) {
    return t("settings.webdavErrorExport");
  }
  if (/^WebDAV (list|test|upload|download|delete) failed:/i.test(message)) {
    return t("settings.webdavErrorRemote");
  }
  return t("settings.webdavErrorUnknown");
}
