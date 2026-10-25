import { describe, expect, it } from "vitest";
import { defaultLocalBackupFilename, formatBackupTimestamp } from "../lib/backupTime";

describe("backupTime", () => {
  it("formats GMT WebDAV timestamps in the local timezone", () => {
    const gmt = "Wed, 19 Aug 2026 13:56:38 GMT";
    const formatted = formatBackupTimestamp(gmt);

    expect(formatted).not.toMatch(/GMT/i);
    expect(formatted).toBe(
      new Date(gmt).toLocaleString(undefined, {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      })
    );
  });

  it("keeps unparseable timestamps unchanged", () => {
    expect(formatBackupTimestamp("not-a-date")).toBe("not-a-date");
  });

  it("names local backup files with the system clock", () => {
    expect(defaultLocalBackupFilename(new Date(2026, 7, 19, 21, 56, 38))).toBe(
      "arsenal-backup-2026-08-19-215638.zip"
    );
  });
});
