import { describe, expect, it } from "vitest";
import { webDavErrorDetail } from "@/lib/webdavError";

function t(key: string) {
  return key;
}

describe("webDavErrorDetail", () => {
  it.each([
    ["WebDAV backup download interrupted; no restore was started", "backupErrorDownloadIncomplete"],
    ["Backup archive entry is corrupt, incomplete, or cannot be staged", "backupErrorCorrupt"],
    ["Backup archive is not a valid ZIP file", "backupErrorCorrupt"],
    ["Backup contains duplicate archive paths", "backupErrorUnsafe"],
    ["Backup expanded size exceeds limit (16 GiB)", "backupErrorExpandedTooLarge"],
    ["Backup manifest exceeds size limit (128 MiB)", "backupErrorManifestTooLarge"],
    ["Failed to create backup staging directory: no space left", "backupErrorStaging"],
    ["Backup restore worker failed; check available disk space and memory", "backupErrorWorker"],
  ])("explains restore failure %s", (error, key) => {
    expect(webDavErrorDetail(t, error)).toBe(`settings.${key}`);
  });
  it("maps object-shaped Tauri errors instead of falling back to unknown", () => {
    expect(
      webDavErrorDetail(t, { message: "WebDAV upload failed with status 409 Conflict" })
    ).toBe("settings.webdavErrorRemote");
  });

  it("maps connection-style transport failures", () => {
    expect(webDavErrorDetail(t, { message: "WebDAV upload failed: error sending request" })).toBe(
      "settings.webdavErrorConnection"
    );
  });

  it("maps oversized backup errors", () => {
    expect(webDavErrorDetail(t, "WebDAV backup exceeds size limit")).toBe(
      "settings.webdavErrorTooLarge"
    );
  });
  it("distinguishes archive download timeout from control request timeout", () => {
    expect(webDavErrorDetail(t, "WebDAV download failed: request timed out")).toBe("settings.webdavErrorDownloadTimeout");
    expect(webDavErrorDetail(t, "WebDAV list failed: request timed out")).toBe("settings.webdavErrorTimeout");
  });

});
