import { describe, it, expect } from "vitest";
import { detect, detectNames } from "./maskEngine";

const names = (t: string) => detectNames(t).map((s) => s.text);

describe("detectNames — moderate corroboration (loosened)", () => {
  it("does NOT mask a lone unknown capitalised word", () => {
    // "Xerxes" is capitalised but not a known name, has no honorific, and is not
    // part of a 2+ run → must not be auto-masked anymore.
    expect(names("Please review Xerxes today")).not.toContain("Xerxes");
  });

  it("masks a known Pakistani name even when alone (gazetteer)", () => {
    expect(names("the case of Bilal is pending")).toContain("Bilal");
    // lowercase gazetteer name too
    expect(names("applicant shahid was late")).toContain("shahid");
  });

  it("masks a single unknown name right after an honorific/trigger", () => {
    expect(names("Mr. Xerxes attended")).toContain("Xerxes");
    expect(names("the applicant Zephyrus objected")).toContain("Zephyrus");
  });

  it("masks a 2+ word capitalised run (likely full name)", () => {
    expect(names("Xerxes Ptolemy signed the note")).toContain("Xerxes Ptolemy");
  });

  it("still does not mask legal/govt/place terms", () => {
    const n = names("the Finance Department in Karachi under Rule 16");
    expect(n).not.toContain("Finance");
    expect(n).not.toContain("Karachi");
    expect(n).not.toContain("Rule");
  });
});

describe("detect — structured PII unaffected by the name change", () => {
  it("still masks CNIC, phone, and money", () => {
    const spans = detect("send Rs 50,000 to 0300-1234567, CNIC 42101-1234567-1");
    const types = spans.map((s) => s.type);
    expect(types).toContain("cnic");
    expect(types).toContain("phone");
    expect(types).toContain("money");
  });
});
