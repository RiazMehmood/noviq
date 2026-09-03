#!/usr/bin/env python3
"""Build the 'stale law' case-study Word document for the Noviq beta pitch.

Honest framing note, deliberately preserved in the output itself: an extensive
search did NOT surface a single named, reported case in the form "executive
decision X was invalidated because the department missed amendment Y." Rather
than stretch an unrelated judgment to fit, the document is built on three
documented, dated chains plus one FIRST-HAND verification that anyone can
reproduce in two minutes — which is more persuasive to a sceptical audience than
a press clipping.

Run:  python scripts/build_currency_case_doc.py <output.docx>
"""

import sys
from docx import Document
from docx.shared import Pt, Inches, RGBColor
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.enum.table import WD_TABLE_ALIGNMENT

NAVY = RGBColor(0x0B, 0x2E, 0x4A)
CRIMSON = RGBColor(0x9B, 0x1C, 0x2E)
TEAL = RGBColor(0x0F, 0x76, 0x6E)
GREY = RGBColor(0x55, 0x60, 0x6B)


def style(doc):
    n = doc.styles["Normal"]
    n.font.name = "Calibri"
    n.font.size = Pt(11)
    n.paragraph_format.space_after = Pt(8)
    n.paragraph_format.line_spacing = 1.15
    for s in doc.sections:
        s.top_margin = s.bottom_margin = Inches(0.9)
        s.left_margin = s.right_margin = Inches(1.0)


def h(doc, txt, size=16, colour=NAVY, before=16, after=6):
    p = doc.add_paragraph()
    p.paragraph_format.space_before = Pt(before)
    p.paragraph_format.space_after = Pt(after)
    r = p.add_run(txt)
    r.bold = True
    r.font.size = Pt(size)
    r.font.color.rgb = colour
    return p


def para(doc, txt, size=11, bold=False, colour=None, italic=False, after=8):
    p = doc.add_paragraph()
    p.paragraph_format.space_after = Pt(after)
    r = p.add_run(txt)
    r.bold = bold
    r.italic = italic
    r.font.size = Pt(size)
    if colour:
        r.font.color.rgb = colour
    return p


def bullet(doc, txt, bold_lead=None):
    p = doc.add_paragraph(style="List Bullet")
    p.paragraph_format.space_after = Pt(4)
    if bold_lead:
        r = p.add_run(bold_lead); r.bold = True; r.font.size = Pt(11)
    r = p.add_run(txt); r.font.size = Pt(11)
    return p


def callout(doc, title, body, colour=CRIMSON):
    t = doc.add_table(rows=1, cols=1)
    t.style = "Table Grid"
    t.alignment = WD_TABLE_ALIGNMENT.CENTER
    c = t.cell(0, 0)
    c.text = ""
    p1 = c.paragraphs[0]
    r = p1.add_run(title); r.bold = True; r.font.size = Pt(11); r.font.color.rgb = colour
    p2 = c.add_paragraph()
    r2 = p2.add_run(body); r2.font.size = Pt(10.5)
    doc.add_paragraph().paragraph_format.space_after = Pt(4)
    return t


def table(doc, headers, rows, widths=None):
    t = doc.add_table(rows=1, cols=len(headers))
    t.style = "Light Grid Accent 1"
    for i, htxt in enumerate(headers):
        cell = t.rows[0].cells[i]
        cell.text = ""
        r = cell.paragraphs[0].add_run(htxt)
        r.bold = True; r.font.size = Pt(10)
    for row in rows:
        cells = t.add_row().cells
        for i, v in enumerate(row):
            cells[i].text = ""
            r = cells[i].paragraphs[0].add_run(str(v))
            r.font.size = Pt(10)
    if widths:
        for i, w in enumerate(widths):
            for row in t.rows:
                row.cells[i].width = Inches(w)
    doc.add_paragraph().paragraph_format.space_after = Pt(4)
    return t


def build(path):
    doc = Document()
    style(doc)

    # ── cover ────────────────────────────────────────────────────────────────
    p = doc.add_paragraph(); p.alignment = WD_ALIGN_PARAGRAPH.LEFT
    r = p.add_run("When the law moves and the file does not")
    r.bold = True; r.font.size = Pt(24); r.font.color.rgb = NAVY
    para(doc, "Documented cases of decisions taken against superseded law — and "
              "what they cost", size=13, colour=GREY)
    para(doc, "Supporting note for the Noviq beta programme  ·  Government of "
              "Sindh  ·  July 2026", size=10, colour=GREY, after=18)

    # ── scope note ───────────────────────────────────────────────────────────
    callout(doc, "A note on what this document does and does not claim",
            "An extensive search did not surface a single named, publicly reported "
            "case in the form “executive decision X was invalidated because the "
            "department had missed amendment Y”. Departments do not publicise such "
            "findings, and where they are litigated the judgment usually records "
            "the legal defect rather than the administrative cause.\n\n"
            "Rather than stretch an unrelated judgment to fit, this note sets out "
            "three documented, dated chains where the law changed and the record "
            "did not follow — including one that the reader can verify personally "
            "in about two minutes. That verification is the strongest item here, "
            "because it is not a claim about someone else's department.",
            colour=NAVY)

    # ── executive summary ────────────────────────────────────────────────────
    h(doc, "In one paragraph")
    para(doc, "A rule can be struck down by the Supreme Court, deleted by the "
              "Cabinet, and still sit — unmarked and authoritative-looking — in the "
              "copy an officer opens, including on the website of the very court "
              "that struck it down. A procurement threshold can change and the old "
              "figure keep circulating for four years while auditors record the same "
              "violation “year after year”. A rule of promotion can change three "
              "weeks ago and every case now in process be wrong. None of these needs "
              "anyone to be careless. They need only that nobody was told.",
              size=12)

    # ═════════ CASE 1 ════════════════════════════════════════════════════════
    h(doc, "Case 1 — A rule struck down, deleted, and still published")
    para(doc, "Sindh Civil Servants (Appointment, Promotion and Transfer) Rules 1974, "
              "Rule 11-A (deceased quota)", bold=True, colour=TEAL)

    table(doc, ["Date", "What happened"], [
        ["26 Sep 2024", "Supreme Court, in General Post Office, Islamabad v. Muhammad "
                        "Jalal (Civil Petition No. 3390 of 2021, reported PLD 2024 SC "
                        "1276), declared Rule 11-A ultra vires and unconstitutional — "
                        "appointment on deceased quota without open advertisement, "
                        "competition and merit."],
        ["4 Dec 2024", "The Sindh Cabinet approved deletion of Rule 11-A from the APT "
                       "Rules 1974, in compliance with that judgment."],
        ["2025", "In Zahida Parveen v. Government of Khyber Pakhtunkhwa (2025 SCP 107) "
                 "the Supreme Court clarified that PLD 2024 SC 1276 operates "
                 "PROSPECTIVELY and “does not take away the accrued rights and/or set "
                 "aside the past and closed transactions.”"],
        ["1 Sep 2025", "In Province of Sindh v. Azhar Ali the Supreme Court dismissed "
                       "the Sindh government's appeals over the deceased-quota "
                       "deadline, holding that beneficial provisions must be read "
                       "“with generous approach”. The province had litigated to the "
                       "apex court and lost."],
        ["10 Mar 2026", "The Federal Constitutional Court rejected a Sindh government "
                        "petition against an appointment on deceased quota."],
    ], widths=[1.2, 5.1])

    h(doc, "The part you can verify yourself", size=12, colour=CRIMSON, before=10)
    para(doc, "Nearly two years after the rule was struck down and eighteen months "
              "after the Cabinet deleted it, Rule 11-A is still present, in full and "
              "unmarked, in the copy of the APT Rules 1974 published on the Sindh "
              "High Court's own website.")
    para(doc, "This was verified directly on 26 July 2026. The published PDF was "
              "downloaded and its text extracted. Rule 11-A appears at line 656, "
              "carrying a footnote recording only its 2011 substitution — nothing "
              "about the judgment that voided it, and nothing about the Cabinet "
              "decision that deleted it.")
    para(doc, "The same file was compared against a second widely-circulated copy of "
              "the same rules. The extracted text of the two was byte-identical "
              "(matching MD5 checksum). In other words, the copies in circulation "
              "agree with one another — and all of them are wrong.", after=10)

    callout(doc, "Why this is the strongest evidence in this note",
            "Every other example here asks you to accept a report about some other "
            "department. This one does not. Open the Sindh High Court's published "
            "copy of the APT Rules 1974, search for “11-A”, and you will find a rule "
            "the Supreme Court voided on 26 September 2024 — presented exactly as it "
            "was before.\n\n"
            "An officer consulting the most authoritative source available to them "
            "would proceed on a rule that no longer exists, and nothing on the page "
            "would warn them.")

    h(doc, "What it cost", size=12, before=10)
    bullet(doc, "Litigation in both directions — the province refusing claims it "
                "should have honoured, and processing claims it should not have.",
           "Litigation. ")
    bullet(doc, "The prospective-effect nuance means an officer must hold three "
                "facts at once: the rule is gone; the judgment is not retrospective; "
                "rights accrued before it survive. Getting any one wrong produces a "
                "defective order.", "Complexity. ")
    bullet(doc, "Cases reached the Supreme Court and the Federal Constitutional "
                "Court, both of which went against the province.", "Escalation. ")

    # ═════════ CASE 2 ════════════════════════════════════════════════════════
    h(doc, "Case 2 — A threshold that changed, and a number that kept circulating")
    para(doc, "Rule 17(1), Sindh Public Procurement Rules 2010 — advertisement "
              "threshold", bold=True, colour=TEAL)

    table(doc, ["", "Text"], [
        ["Superseded", "“Procurements over one hundred thousand rupees and up to one "
                       "million rupees shall be advertised by timely notifications on "
                       "the Authority's website and may in print media…”"],
        ["In force since 15 July 2022", "“Procurements over three hundred thousand "
                                        "rupees and up to two million rupees shall be "
                                        "advertised by timely notifications on the "
                                        "Authority's website and may in print media…”"],
    ], widths=[1.7, 4.6])

    para(doc, "The Auditor-General's audit of Sindh Union Committees and Councils "
              "recorded violation of the advertisement rules in 14 cases involving "
              "Rs 194.884 million, described the pre-audit function as “almost "
              "non-functional”, and — the finding that matters most — noted that "
              "“similar nature of internal control violations reported year after "
              "year.”")
    para(doc, "That phrase is the whole argument. A violation that recurs annually "
              "is not a contested rule. It is a rule whose current text is not "
              "reaching the officer at the moment of decision.", bold=True, after=10)

    callout(doc, "Verified in our own system, and worth admitting",
            "When the Noviq corpus was audited on 26 July 2026, it was found to hold "
            "the 2013 revision of these rules — and therefore to have been answering "
            "this question with the superseded Rs 100,000 to Rs 1,000,000 figure for "
            "four years. It has since been replaced with the text as amended to "
            "15 July 2022.\n\n"
            "This is included deliberately. A system that claims currency must be "
            "able to show how it detects and corrects its own staleness. Ours was "
            "found by audit, corrected, and the audit is now part of deployment.",
            colour=NAVY)

    # ═════════ CASE 3 ════════════════════════════════════════════════════════
    h(doc, "Case 3 — A rule of promotion that changed three weeks ago")
    para(doc, "Supreme Court of Pakistan, 3 July 2026", bold=True, colour=TEAL)
    para(doc, "A three-member bench, deciding two civil appeals against a Sindh "
              "Service Tribunal decision, held that service rendered on a contractual "
              "basis cannot be counted toward the qualifying service required for "
              "promotion: contract employees fall outside the definition of “civil "
              "servant” under the Sindh Civil Servants Act 1973.")
    para(doc, "The Court added that “an unlawful benefit mistakenly extended to one "
              "employee in the past did not create a legal right for others to claim "
              "the same concession.”", italic=True)
    para(doc, "Two consequences follow immediately. Any promotion case in process "
              "that counts contract service is now being decided under superseded "
              "law. And “this is how we have always done it” has been expressly "
              "rejected as a defence — so past practice offers no protection.",
              after=10)

    callout(doc, "This is the scenario in its live form",
            "This judgment is weeks old. Somewhere in the province, a Departmental "
            "Promotion Committee is meeting with a seniority list prepared before it. "
            "No officer can be expected to track Supreme Court judgments in real time "
            "across every service rule while also clearing a file. A system can.",
            colour=NAVY)

    # ── the pattern ──────────────────────────────────────────────────────────
    h(doc, "The pattern across all three")
    table(doc, ["", "Case 1", "Case 2", "Case 3"], [
        ["What changed", "Rule struck down and deleted", "Threshold raised",
         "Qualifying service redefined"],
        ["When", "Sep–Dec 2024", "15 July 2022", "3 July 2026"],
        ["Was it published?", "Yes", "Yes", "Yes"],
        ["Did the record follow?", "No — still in the SHC's own copy",
         "Not in practice", "Too recent to have"],
        ["Anyone at fault?", "No", "No", "No"],
    ], widths=[1.5, 1.7, 1.5, 1.6])

    para(doc, "In none of these did anyone act carelessly. The rule was public in "
              "every case. What failed was distribution — the interval between a law "
              "changing and the officer who applies it finding out. That interval is "
              "measured in months or years, and during it every decision taken is "
              "taken against superseded law.", size=12, bold=True)

    # ── what Noviq does / does not ───────────────────────────────────────────
    h(doc, "What Noviq changes — and what it does not")
    para(doc, "What it changes", bold=True, colour=TEAL, after=4)
    bullet(doc, "One corpus for the department, so an amendment is applied once and "
                "everybody sees it — rather than each officer maintaining a private "
                "folder of PDFs of unknown vintage.", "Single source. ")
    bullet(doc, "Every instrument carries the date it is amended to, and that date is "
                "shown with the answer. An officer can see the currency and judge it. "
                "Silent staleness is the danger; visible staleness is a limitation "
                "they can work with.", "Visible currency. ")
    bullet(doc, "Where a court has struck down a provision the corpus holds, that "
                "fact belongs next to the text — which is exactly what the published "
                "copies in Case 1 do not do.", "Judgments attached. ")
    bullet(doc, "A currency review runs before each deployment, and the department's "
                "IT staff are trained to maintain it afterwards.", "Reviewed. ")

    para(doc, "What it does not change", bold=True, colour=CRIMSON, after=4)
    bullet(doc, "It cannot know about an amendment nobody has loaded. If the "
                "notification never reaches the department, Noviq is as blind as the "
                "officer — the difference is that it will show you the date it is "
                "current to, so the gap is visible rather than assumed away.")
    bullet(doc, "It does not decide contested questions of interpretation, and it "
                "does not replace the Law Department.")
    bullet(doc, "It does not fix a rule that is known and deliberately overridden. "
                "That is a different problem and no software solves it.")

    # ── sources ──────────────────────────────────────────────────────────────
    h(doc, "Sources")
    srcs = [
        "General Post Office, Islamabad v. Muhammad Jalal, PLD 2024 SC 1276 "
        "(Civil Petition No. 3390 of 2021), decided 26 September 2024.",
        "Zahida Parveen v. Government of Khyber Pakhtunkhwa, 2025 SCP 107 — "
        "prospective effect — https://sahsol.lums.edu.pk/node/24186",
        "Province of Sindh through Chief Secretary v. Azhar Ali, decided "
        "1 September 2025 — https://pakistankanoon.com/sclr2026.php/2026-SCLR-6  "
        "(the reporting year appears as both 2025 and 2026 across sources; verify "
        "the citation before quoting it formally)",
        "The Express Tribune — “Deceased quota no more” — "
        "https://tribune.com.pk/story/2513859/deceased-quota-no-more",
        "Business Recorder — Sindh cabinet abolishes deceased quota — "
        "https://www.brecorder.com/news/40335984/sindh-cabinet-abolishes-deceased-quota-reforms-educational-boards",
        "The Nation — FCC rejects Sindh govt petition on deceased quota, "
        "10 March 2026 — "
        "https://www.nation.com.pk/10-Mar-2026/fcc-rejects-sindh-govt-petition-appointment-deceased-quota",
        "Sindh Civil Servants (APT) Rules 1974, as published by the Sindh High Court "
        "— https://sindhhighcourt.gov.pk/laws/SINDH%20CIVIL%20SERVANTS%20"
        "(APPOINTMENT,%20PROMOTION%20AND%20TRANSFER)%20RULES,%201974.pdf  "
        "(contains Rule 11-A as at 26 July 2026)",
        "Sindh Public Procurement Rules 2010, amended to 15 July 2022 — "
        "https://www.pprasindh.gov.pk/SPPRARulesAmended201015072022.pdf",
        "Auditor-General of Pakistan — audit report on Union Committees / Councils "
        "of Sindh — https://agp.gov.pk/SiteImage/Policy/FINAL-UC-2017-18.pdf",
        "The Nation — “Contract service cannot be counted towards promotion, rules "
        "SC”, 3 July 2026 — "
        "https://www.nation.com.pk/03-Jul-2026/contract-service-cannot-counted-towards-promotion-rules-sc",
    ]
    for s in srcs:
        p = doc.add_paragraph(style="List Number")
        p.paragraph_format.space_after = Pt(5)
        r = p.add_run(s); r.font.size = Pt(9.5)

    # ── presenter caution ────────────────────────────────────────────────────
    doc.add_page_break()
    h(doc, "Presenter caution — read before using Case 1", colour=CRIMSON)
    para(doc, "Case 1 is the most persuasive item in this note and also the most "
              "dangerous to present carelessly.")
    bullet(doc, "Noviq's own corpus still contains Rule 11-A. Until the SGA&CD "
                "deletion notification is obtained and loaded, do NOT ask Noviq a "
                "deceased-quota question in a live demonstration. It will answer "
                "from the rule as published — which is the very failure being "
                "described.", "Do not demonstrate it. ")
    bullet(doc, "Serving officers in the room may have processed the fallout "
                "personally. Present the stale published copy as a systemic problem "
                "affecting everyone, never as a criticism of a department.",
           "Do not make it accusatory. ")
    bullet(doc, "The deletion notification is not published online. Requesting it "
                "from SGA&CD is both the fix for the corpus and a reasonable "
                "discovery question to put to the client.", "Ask for the notification. ")
    bullet(doc, "Separately: confirm whether the Removal from Service (Special "
                "Powers) Sindh Ordinance 2000 has actually been repealed before "
                "presenting any disciplinary scenario as current law. The federal "
                "ordinance was repealed by Act III of 2010; the Sindh position could "
                "not be confirmed from public sources.", "Unresolved. ")

    doc.save(path)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "currency-case-study.docx"
    build(out)
    print(f"written: {out}")
