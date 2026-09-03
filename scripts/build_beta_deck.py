#!/usr/bin/env python3
"""Generate the Noviq beta-launch deck for the Sindh Secretariat.

Content is grounded in docs/beta-launch/{research-personas-and-painpoints,
evidence-dossier-documented-cases,corpus-currency-and-scoping-audit}.md — every
figure and case on a slide traces to a cited source in those files.

Deliberate content choices, so they are not "tidied away" by a later edit:
  * The pitch leads with LEGAL EXPOSURE, not efficiency. For a Secretary an order
    that survives challenge is worth more than minutes saved.
  * Slide 3 pre-empts the e-DAFTER objection. Sindh is implementing e-Dafter
    (SGA&CD with Sukkur IBA) for digital file movement — if Noviq sounds like it,
    the meeting ends there. Slide 20 then turns it into the continuity argument:
    e-Dafter moves the file, Noviq answers what the file is about. Do NOT
    reintroduce federal framing; this audience is provincial.
  * There is a "What Noviq does NOT do" slide. Presenting a patronage problem
    (OPS postings) as something a search tool fixes would be torn apart by anyone
    in that room; saying so plainly is what makes the rest credible.
  * NO accuracy percentages anywhere. We have not built generation-based evals,
    so any number would be invented.
  * Deceased quota / Rule 11-A is absent by design: the Supreme Court struck the
    rule down (26 Sep 2024) and the Sindh Cabinet deleted it (04.12.2024), but our
    corpus copy — and the Sindh High Court's own published copy — still contain it.
  * Screenshot slides are PLACEHOLDERS with the exact question to run. They must
    be captured from the live system before this is presented.

Run:  python scripts/build_beta_deck.py [output.pptx]
"""

import sys
from pptx import Presentation
from pptx.util import Inches, Pt, Emu
from pptx.dml.color import RGBColor
from pptx.enum.text import PP_ALIGN, MSO_ANCHOR

# ── palette ──────────────────────────────────────────────────────────────────
NAVY   = RGBColor(0x0B, 0x2E, 0x4A)
INK    = RGBColor(0x1A, 0x1D, 0x21)
SLATE  = RGBColor(0x55, 0x60, 0x6B)
MUTED  = RGBColor(0x8A, 0x93, 0x9C)
TEAL   = RGBColor(0x0F, 0x76, 0x6E)
AMBER  = RGBColor(0xB4, 0x7E, 0x00)
CRIMSON= RGBColor(0x9B, 0x1C, 0x2E)
PAPER  = RGBColor(0xFA, 0xFB, 0xFC)
LINE   = RGBColor(0xD5, 0xDC, 0xE2)
WHITE  = RGBColor(0xFF, 0xFF, 0xFF)

W, H = Inches(13.333), Inches(7.5)


def deck():
    p = Presentation()
    p.slide_width, p.slide_height = W, H
    return p


def blank(prs):
    return prs.slides.add_slide(prs.slide_layouts[6])


def box(slide, x, y, w, h, fill=None, line=None, lw=1.0):
    from pptx.enum.shapes import MSO_SHAPE
    s = slide.shapes.add_shape(MSO_SHAPE.RECTANGLE, x, y, w, h)
    if fill is None:
        s.fill.background()
    else:
        s.fill.solid(); s.fill.fore_color.rgb = fill
    if line is None:
        s.line.fill.background()
    else:
        s.line.color.rgb = line; s.line.width = Pt(lw)
    s.shadow.inherit = False
    return s


def text(slide, x, y, w, h, runs, align=PP_ALIGN.LEFT, anchor=MSO_ANCHOR.TOP,
         spacing=1.0):
    """runs = [(string, size_pt, bold, color, space_after_pt), ...]"""
    tb = slide.shapes.add_textbox(x, y, w, h)
    tf = tb.text_frame
    tf.word_wrap = True
    tf.vertical_anchor = anchor
    tf.margin_left = tf.margin_right = tf.margin_top = tf.margin_bottom = 0
    first = True
    for item in runs:
        s, sz, b, col, after = item
        para = tf.paragraphs[0] if first else tf.add_paragraph()
        first = False
        para.alignment = align
        para.space_after = Pt(after)
        para.line_spacing = spacing
        r = para.add_run(); r.text = s
        r.font.size = Pt(sz); r.font.bold = b; r.font.color.rgb = col
        r.font.name = "Segoe UI"
    return tb


def header(slide, kicker, title, sub=None):
    box(slide, 0, 0, W, Inches(1.35), fill=NAVY)
    text(slide, Inches(0.7), Inches(0.24), Inches(12), Inches(0.35),
         [(kicker.upper(), 11, True, RGBColor(0x7F, 0xB3, 0xD5), 2)])
    text(slide, Inches(0.7), Inches(0.56), Inches(12), Inches(0.6),
         [(title, 27, True, WHITE, 0)])
    if sub:
        text(slide, Inches(0.7), Inches(1.55), Inches(12), Inches(0.4),
             [(sub, 14, False, SLATE, 0)])


def footer(slide, n):
    text(slide, Inches(0.7), Inches(7.02), Inches(8), Inches(0.3),
         [("Noviq  ·  Beta programme  ·  Government of Sindh", 9, False, MUTED, 0)])
    text(slide, Inches(11.4), Inches(7.02), Inches(1.2), Inches(0.3),
         [(str(n), 9, False, MUTED, 0)], align=PP_ALIGN.RIGHT)


def bullets(slide, x, y, w, items, size=15, gap=11, colour=INK, bullet="—"):
    runs = []
    for it in items:
        if isinstance(it, tuple):
            lead, rest = it
            runs.append((f"{bullet}  {lead}{rest}", size, False, colour, gap))
        else:
            runs.append((f"{bullet}  {it}", size, False, colour, gap))
    return text(slide, x, y, w, Inches(4.5), runs, spacing=1.18)


def stat(slide, x, y, w, big, label, colour=NAVY):
    box(slide, x, y, w, Inches(1.55), fill=PAPER, line=LINE)
    text(slide, x + Inches(0.25), y + Inches(0.18), w - Inches(0.5), Inches(0.6),
         [(big, 30, True, colour, 2)])
    text(slide, x + Inches(0.25), y + Inches(0.82), w - Inches(0.5), Inches(0.6),
         [(label, 11, False, SLATE, 0)], spacing=1.12)


def shot(slide, x, y, w, h, question, note):
    """Placeholder for a screenshot that MUST be captured from the live system."""
    box(slide, x, y, w, h, fill=RGBColor(0xF2, 0xF5, 0xF8), line=LINE)
    text(slide, x + Inches(0.3), y + Inches(0.25), w - Inches(0.6), Inches(0.4),
         [("SCREENSHOT TO CAPTURE", 10, True, CRIMSON, 8)])
    text(slide, x + Inches(0.3), y + Inches(0.62), w - Inches(0.6), Inches(1.2),
         [(f"“{question}”", 15, True, INK, 8)], spacing=1.15)
    text(slide, x + Inches(0.3), y + h - Inches(0.85), w - Inches(0.6), Inches(0.7),
         [(note, 10, False, SLATE, 0)], spacing=1.15)


def flow(slide, x, y, steps, w_each=Inches(2.25), h=Inches(0.95)):
    gap = Inches(0.28)
    for i, (t, s) in enumerate(steps):
        bx = x + i * (w_each + gap)
        box(slide, bx, y, w_each, h, fill=WHITE, line=NAVY, lw=1.25)
        text(slide, bx + Inches(0.12), y + Inches(0.14), w_each - Inches(0.24),
             h - Inches(0.28),
             [(t, 11, True, NAVY, 3), (s, 9.5, False, SLATE, 0)], spacing=1.1)
        if i < len(steps) - 1:
            text(slide, bx + w_each + Inches(0.02), y + Inches(0.28),
                 gap, Inches(0.4), [("›", 20, True, MUTED, 0)],
                 align=PP_ALIGN.CENTER)


def tag(slide, x, y, label, fg, bg):
    w = Inches(0.16 + 0.098 * len(label))
    box(slide, x, y, w, Inches(0.3), fill=bg, line=None)
    text(slide, x, y + Inches(0.055), w, Inches(0.25),
         [(label, 9.5, True, fg, 0)], align=PP_ALIGN.CENTER)
    return w


# ═════════════════════════════════════════════════════════════════════════════
def build(path):
    prs = deck()
    n = 0

    # ── 1 title ──────────────────────────────────────────────────────────────
    s = blank(prs); box(s, 0, 0, W, H, fill=NAVY)
    text(s, Inches(1.1), Inches(2.15), Inches(11), Inches(0.5),
         [("BETA PROGRAMME", 13, True, RGBColor(0x7F, 0xB3, 0xD5), 10)])
    text(s, Inches(1.1), Inches(2.6), Inches(11), Inches(1.2),
         [("Noviq", 54, True, WHITE, 6)])
    text(s, Inches(1.1), Inches(3.55), Inches(10.5), Inches(1.0),
         [("The rule that applies, with the citation attached.", 24, False,
           RGBColor(0xC8, 0xD8, 0xE4), 0)], spacing=1.15)
    box(s, Inches(1.1), Inches(4.7), Inches(2.2), Pt(3), fill=TEAL)
    text(s, Inches(1.1), Inches(5.1), Inches(11), Inches(0.8),
         [("Prepared for the Government of Sindh Secretariat", 14, False,
           RGBColor(0x9F, 0xB8, 0xCC), 4)])

    # ── 2 the gap ────────────────────────────────────────────────────────────
    n += 1; s = blank(prs); header(s, "The problem", "Your rules are not in doubt.")
    text(s, Inches(0.7), Inches(1.95), Inches(11.9), Inches(2.6),
         [("Your officers are capable. But knowing WHICH rule applies — in what "
           "version, at what threshold, under which of two overlapping instruments "
           "— is scattered across twelve instruments, and it walks out of the "
           "building every eleven months.", 25, False, INK, 16)], spacing=1.3)
    box(s, Inches(0.7), Inches(4.35), Inches(11.9), Pt(2), fill=LINE)
    text(s, Inches(0.7), Inches(4.7), Inches(11.9), Inches(1.0),
         [("That gap is what produces set-aside orders, the same audit paragraphs "
           "year after year, and litigation you should have won.", 19, True, NAVY, 0)],
         spacing=1.25)
    footer(s, n)

    # ── 3 e-Dafter pre-empt ─────────────────────────────────────────────────
    # The single most important slide for this audience. Sindh has committed to
    # e-Dafter (SGA&CD with Sukkur IBA). If Noviq sounds like it, the meeting ends.
    n += 1; s = blank(prs)
    header(s, "First, what this is not", "You are building e-Dafter. This is not that.")
    box(s, Inches(0.7), Inches(1.95), Inches(5.7), Inches(3.6), fill=PAPER, line=LINE)
    text(s, Inches(1.0), Inches(2.2), Inches(5.1), Inches(3.2),
         [("e-Dafter", 20, True, SLATE, 10),
          ("Moves the file.", 16, True, INK, 14),
          ("Digital file movement and real-time tracking, device-bound access, "
           "digital signatures, secure document storage.", 13, False, SLATE, 12),
          ("It does not tell you which rule applies to what is inside the file.",
           13, True, INK, 0)], spacing=1.2)
    box(s, Inches(6.9), Inches(1.95), Inches(5.7), Inches(3.6), fill=WHITE, line=NAVY, lw=1.5)
    text(s, Inches(7.2), Inches(2.2), Inches(5.1), Inches(3.2),
         [("Noviq", 20, True, TEAL, 10),
          ("Answers the question the file is about.", 16, True, INK, 14),
          ("Which instrument governs. Which sub-rule. What threshold. "
           "What the current text actually says.", 13, False, SLATE, 12),
          ("With the provision quoted and cited, so the note can be checked.",
           13, True, INK, 0)], spacing=1.2)
    text(s, Inches(0.7), Inches(5.8), Inches(11.9), Inches(0.7),
         [("The two are complementary. Noviq does not replace, duplicate or "
           "compete with e-Dafter — it answers the question the file carries.",
           15, True, NAVY, 0)])
    footer(s, n)

    # ── 4 personas ──────────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "Who this is for", "Five people, one file.")
    rows = [
        ("Section Officer", "Examines the case, writes the noting and the draft.",
         "Which of twelve instruments governs this? Is the provision current? "
         "Will my note survive the auditor?"),
        ("Deputy Secretary", "Chief of a group of sections; everything upward routes through this office.",
         "I must catch a subordinate's error without redoing the research myself."),
        ("Secretary", "Administratively accountable for the department.",
         "Audit paragraphs, litigation, set-aside orders, inconsistent "
         "interpretation between my own sections."),
        ("Addl. Chief Secretary", "Apex of the provincial service; SGA&CD owns postings.",
         "Knowledge is lost at every rotation. Nothing is consistent across departments."),
        ("Provincial Minister", "Politically accountable for the outcome.",
         "Delay, and decisions I cannot defend in public."),
    ]
    y = Inches(1.9)
    for i, (who, role, pain) in enumerate(rows):
        bg = PAPER if i % 2 == 0 else WHITE
        box(s, Inches(0.7), y, Inches(11.9), Inches(0.94), fill=bg, line=LINE)
        text(s, Inches(0.95), y + Inches(0.14), Inches(2.5), Inches(0.7),
             [(who, 13.5, True, NAVY, 2), (role, 9, False, MUTED, 0)], spacing=1.1)
        text(s, Inches(3.7), y + Inches(0.26), Inches(8.6), Inches(0.6),
             [(pain, 12.5, False, INK, 0)], spacing=1.15)
        y += Inches(0.98)
    text(s, Inches(0.7), Inches(6.85), Inches(11.9), Inches(0.4),
         [("The officer at the top of this list decides whether Noviq is adopted. "
           "The officer at the bottom decides whether it is used.", 12, True, SLATE, 0)])
    footer(s, n)

    # ── 5 what it costs ─────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "What the gap costs", "This is not a productivity problem.")
    stat(s, Inches(0.7), Inches(2.0), Inches(3.7), "Vitiated",
         "Sindh High Court: proceedings taken “under a misconception of law and "
         "under a wrong law” vitiated ENTIRELY — charge sheet, inquiry, "
         "penalty, final order.", CRIMSON)
    stat(s, Inches(4.8), Inches(2.0), Inches(3.7), "Year after year",
         "AGP audit of Sindh Union Committees: “similar nature of internal "
         "control violations reported year after year”.", AMBER)
    stat(s, Inches(8.9), Inches(2.0), Inches(3.7), "Rs 194.884m",
         "Violation of rules in 14 cases in that single audit; pre-audit "
         "described as “almost non-functional”.", AMBER)
    text(s, Inches(0.7), Inches(4.0), Inches(11.9), Inches(2.4),
         [("Every one of these is a KNOWLEDGE failure, not a competence failure.",
           19, True, NAVY, 14),
          ("The rule existed. It was published. It was not in dispute. It simply "
           "was not in front of the officer at the moment of decision — and the "
           "consequence landed on the department, not on the rule book.",
           15, False, INK, 0)], spacing=1.25)
    footer(s, n)

    # ── 6 evidence 1 ────────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "Evidence 1 of 3", "The wrong instrument voids the whole proceeding.")
    text(s, Inches(0.7), Inches(1.95), Inches(11.9), Inches(0.9),
         [("Two instruments govern discipline in Sindh. They carry different "
           "procedures and different deadlines.", 16, False, INK, 0)], spacing=1.2)
    box(s, Inches(0.7), Inches(2.95), Inches(5.7), Inches(1.5), fill=PAPER, line=LINE)
    text(s, Inches(0.95), Inches(3.15), Inches(5.2), Inches(1.1),
         [("Sindh Civil Servants (E&D) Rules 1973", 14, True, NAVY, 5),
          ("Explanation → show cause → inquiry officer or committee.",
           12, False, SLATE, 0)], spacing=1.15)
    box(s, Inches(6.9), Inches(2.95), Inches(5.7), Inches(1.5), fill=PAPER, line=LINE)
    text(s, Inches(7.15), Inches(3.15), Inches(5.2), Inches(1.1),
         [("Removal from Service (Special Powers) Sindh Ordinance 2000", 14, True, NAVY, 5),
          ("Inquiry must be completed within 25 DAYS of initiation.",
           12, True, CRIMSON, 0)], spacing=1.15)
    box(s, Inches(0.7), Inches(4.75), Inches(11.9), Inches(1.5), fill=RGBColor(0xFD, 0xF1, 0xF2),
        line=CRIMSON)
    text(s, Inches(1.0), Inches(4.98), Inches(11.3), Inches(1.1),
         [("Departments have run the entire process under the wrong one. "
           "The High Court held that action begun and concluded under a "
           "misconception of law vitiated the proceedings in full — including "
           "the final order. The department starts again; the officer is meanwhile "
           "reinstated.", 14, False, INK, 0)], spacing=1.25)
    text(s, Inches(0.7), Inches(6.45), Inches(11.9), Inches(0.4),
         [("Choosing between the two is a legal question asked at the very start "
           "of a case — usually by a Section Officer, usually without counsel.",
           12.5, True, NAVY, 0)])
    footer(s, n)

    # ── 7 evidence 2 ────────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "Evidence 2 of 3", "The law changed three weeks ago. Did the file know?")
    text(s, Inches(0.7), Inches(2.0), Inches(11.9), Inches(1.0),
         [("3 July 2026 — Supreme Court of Pakistan", 13, True, TEAL, 8),
          ("Contractual service cannot be counted toward the qualifying service "
           "required for promotion. Contract employees fall outside the definition "
           "of “civil servant” under the Sindh Civil Servants Act 1973.",
           17, False, INK, 0)], spacing=1.25)
    box(s, Inches(0.7), Inches(3.5), Inches(11.9), Inches(1.25),
        fill=RGBColor(0xFD, 0xF8, 0xEC), line=AMBER)
    text(s, Inches(1.0), Inches(3.72), Inches(11.3), Inches(0.9),
         [("“An unlawful benefit mistakenly extended to one employee in the "
           "past did not create a legal right for others to claim the same "
           "concession.”", 15, True, INK, 0)], spacing=1.25)
    text(s, Inches(0.7), Inches(5.0), Inches(11.9), Inches(1.5),
         [("“We have always done it this way” is now an expressly "
           "rejected defence.", 16, True, NAVY, 12),
          ("No officer can track Supreme Court judgments in real time across every "
           "service rule while also clearing a file. A system can.", 15, False, INK, 0)],
         spacing=1.25)
    footer(s, n)

    # ── 8 evidence 3 ────────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "Evidence 3 of 3", "A number that has been wrong in practice for years.")
    text(s, Inches(0.7), Inches(1.95), Inches(11.9), Inches(0.5),
         [("Rule 17(1), Sindh Public Procurement Rules 2010 — advertisement threshold",
           14, True, TEAL, 0)])
    box(s, Inches(0.7), Inches(2.55), Inches(5.7), Inches(1.35), fill=PAPER, line=LINE)
    text(s, Inches(0.95), Inches(2.78), Inches(5.2), Inches(0.9),
         [("Superseded text", 11, True, MUTED, 5),
          ("over Rs 100,000  up to  Rs 1,000,000", 17, True, MUTED, 0)], spacing=1.15)
    box(s, Inches(6.9), Inches(2.55), Inches(5.7), Inches(1.35), fill=WHITE, line=TEAL, lw=1.75)
    text(s, Inches(7.15), Inches(2.78), Inches(5.2), Inches(0.9),
         [("In force since 15 July 2022", 11, True, TEAL, 5),
          ("over Rs 300,000  up to  Rs 2,000,000", 17, True, INK, 0)], spacing=1.15)
    bullets(s, Inches(0.7), Inches(4.2), Inches(11.9), [
        "Departments awarded work without inviting open tenders or advertising at all.",
        "The AGP found the same class of violation “year after year”.",
        "The rule is not in dispute. The knowledge of it is simply not reaching the "
        "officer at the moment of decision.",
    ], size=15, gap=13)
    text(s, Inches(0.7), Inches(6.2), Inches(11.9), Inches(0.6),
         [("A precise, numeric, frequently-breached threshold is exactly the kind "
           "of question a retrieval system answers well.", 14, True, NAVY, 0)])
    footer(s, n)

    # ── 9 how it works ──────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "How it works", "Plain language in. Cited provision out.")
    flow(s, Inches(0.7), Inches(2.2), [
        ("Your instruments", "18 Sindh laws, rules,\nmanuals and codes"),
        ("Indexed", "Parsed to the clause,\nnot the page"),
        ("Officer asks", "In plain English\nor Urdu"),
        ("Noviq answers", "Quoting the provision,\nnaming the instrument"),
        ("Officer verifies", "Citation opens the\nsource text"),
    ], w_each=Inches(2.2))
    box(s, Inches(0.7), Inches(3.75), Inches(11.9), Pt(2), fill=LINE)
    text(s, Inches(0.7), Inches(4.1), Inches(5.7), Inches(2.4),
         [("It runs on your own server.", 16, True, NAVY, 10),
          ("Installed in the department. The corpus is yours. Your IT staff are "
           "trained to maintain it.", 14, False, INK, 0)], spacing=1.2)
    text(s, Inches(6.9), Inches(4.1), Inches(5.7), Inches(2.4),
         [("It says when it does not know.", 16, True, NAVY, 10),
          ("If the corpus does not support an answer, Noviq declines rather than "
           "producing a plausible one. A confident wrong citation is the worst "
           "possible failure in this work.", 14, False, INK, 0)], spacing=1.2)
    footer(s, n)

    # ── 10-13 scenarios ─────────────────────────────────────────────────────
    scen = [
        ("Scenario 1", "Disciplinary proceedings",
         "A Section Officer must issue a show-cause notice.",
         [("Officer asks", "Which instrument\ngoverns this case?"),
          ("Noviq cites", "E&D 1973 or RSO 2000\n— and what each requires"),
          ("Officer drafts", "Notice states charges\nand grounds specifically"),
          ("DS reviews", "Checks the citation,\nnot the whole rule book")],
         "Why it matters: a vague notice violates natural justice and vitiates the "
         "process; the wrong instrument vitiates it entirely.",
         "Which rules govern a show-cause notice for a civil servant in Sindh?"),
        ("Scenario 2", "Delegation of financial powers",
         "“Can I sanction this at my level?” — asked more often than "
         "any other question here.",
         [("Officer asks", "Can a BS-19 officer\nsanction this amount?"),
          ("Noviq cites", "The delegation table\nand the limit"),
          ("Officer proceeds", "Or escalates, knowing\nwhy"),
          ("Audit trail", "The rule and citation\nare on the file")],
         "Numeric, auditable, and settled by a single table that few people have "
         "memorised correctly.",
         "What is the sanctioning limit for an officer in BS-19 under the Sindh "
         "Delegation of Financial Powers 2019?"),
        ("Scenario 3", "Procurement threshold",
         "The rule behind repeat audit paragraphs.",
         [("Officer asks", "Must this tender be\nadvertised?"),
          ("Noviq cites", "Rule 17(1) SPPRA 2010,\ncurrent text"),
          ("Officer advertises", "Or documents why\nnot required"),
          ("Auditor finds", "The rule cited on\nthe file")],
         "Removes “I did not know the threshold” as an explanation — "
         "which narrows every future audit paragraph to a deliberate act.",
         "What is the advertisement threshold under Rule 17(1) of the SPPRA Rules?"),
        ("Scenario 4", "Posting, promotion and seniority",
         "The highest-volume daily work in the Secretariat.",
         [("Officer asks", "Who is the competent\nauthority here?"),
          ("Noviq cites", "APT Rules 1974 +\nRules of Business 1986"),
          ("Officer notes", "With the provision\nquoted"),
          ("Secretary signs", "On a defensible\nrecord")],
         "The governing provision is split across instruments — and the same "
         "rule number means different things in different ones.",
         "Who is the competent authority for the transfer of a BS-18 officer?"),
    ]
    for kicker, title, lede, steps, why, q in scen:
        n += 1; s = blank(prs)
        header(s, kicker, title)
        text(s, Inches(0.7), Inches(1.9), Inches(11.9), Inches(0.5),
             [(lede, 15, False, SLATE, 0)])
        flow(s, Inches(0.7), Inches(2.55), steps, w_each=Inches(2.6))
        shot(s, Inches(0.7), Inches(3.95), Inches(7.4), Inches(2.35), q,
             "Capture the real answer with its citation panel from the live system.")
        box(s, Inches(8.5), Inches(3.95), Inches(4.1), Inches(2.35), fill=PAPER, line=LINE)
        text(s, Inches(8.75), Inches(4.2), Inches(3.6), Inches(1.9),
             [("Why it matters", 12, True, NAVY, 8), (why, 12.5, False, INK, 0)],
             spacing=1.2)
        footer(s, n)

    # ── 14 drafting (direction of travel) ───────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "Scenario 5", "Drafting the summary — direction of travel")
    text(s, Inches(0.7), Inches(1.9), Inches(11.9), Inches(0.5),
         [("Shown last, and deliberately framed as where we are heading rather "
           "than what is finished.", 15, False, SLATE, 0)])
    flow(s, Inches(0.7), Inches(2.5), [
        ("Officer opens", "A summary for the\nMinister"),
        ("Noviq supplies", "Governing provisions\nand precedent"),
        ("Officer writes", "The judgement remains\nthe officer's"),
        ("Reviewer checks", "Citations, not\nrecollection"),
    ], w_each=Inches(2.6))
    box(s, Inches(0.7), Inches(4.0), Inches(11.9), Inches(1.6),
        fill=RGBColor(0xFD, 0xF8, 0xEC), line=AMBER)
    text(s, Inches(1.0), Inches(4.25), Inches(11.3), Inches(1.2),
         [("We are not claiming Noviq writes your summaries.", 15, True, INK, 8),
          ("Retrieval with citations is proven and is what the beta is for. "
           "Assisted drafting is under development and will be offered only when "
           "it meets the same evidentiary standard.", 13.5, False, INK, 0)],
         spacing=1.2)
    footer(s, n)

    # ── 15 what Noviq does NOT do ───────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "Being straight with you", "What Noviq does not do.")
    box(s, Inches(0.7), Inches(1.95), Inches(11.9), Inches(1.9),
        fill=RGBColor(0xFD, 0xF1, 0xF2), line=CRIMSON)
    text(s, Inches(1.0), Inches(2.2), Inches(11.3), Inches(1.5),
         [("It does not fix a rule that is known and overridden.", 17, True, INK, 10),
          ("The Supreme Court ordered that junior officers not be posted to senior "
           "posts on OPS. The High Court held the practice a violation of law and "
           "directed that it stop. It continued. No retrieval system changes that.",
           14, False, INK, 0)], spacing=1.25)
    bullets(s, Inches(0.7), Inches(4.15), Inches(11.9), [
        "It does not prevent procurement fraud. It removes ignorance of the "
        "threshold as an available explanation.",
        "It does not decide questions of interpretation, and it does not replace "
        "the Law Department.",
        "It does not audit your files or supervise your officers.",
    ], size=14.5, gap=12)
    box(s, Inches(0.7), Inches(6.1), Inches(11.9), Inches(0.75), fill=PAPER, line=NAVY)
    text(s, Inches(1.0), Inches(6.28), Inches(11.3), Inches(0.5),
         [("What it does change: once the rule and its citation are on the file, a "
           "deviation becomes a documented choice rather than a plausible oversight.",
           14, True, NAVY, 0)])
    footer(s, n)

    # ── 16 the corpus ───────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "What is loaded today", "Eighteen instruments, current as of July 2026.")
    left = ["Constitution of Pakistan (to 27th Amendment)",
            "Constitution (27th Amendment) Act 2025",
            "Sindh Government Rules of Business 1986",
            "Manual of Secretariat Instructions 2010",
            "Sindh Civil Servants Act 1973",
            "Sindh Civil Servants (APT) Rules 1974",
            "Sindh Civil Servants (E&D) Rules 1973",
            "Removal from Service (Sp. Powers) Ordinance 2000",
            "Sindh Service Tribunals Act 1973"]
    right = ["Sindh Public Procurement Act 2009",
             "SPPRA Rules 2010 (amended 15 Jul 2022)",
             "SPPRA Blacklisting Regulations 2023",
             "SPPRA Review Committee Guidelines 2023",
             "SPPRA Policy Letter (Oct 2023)",
             "Sindh Delegation of Financial Powers 2019",
             "Manual for Development Projects 2019 (PC-I)",
             "Sindh Local Government Act 2013",
             "Compendium of Rules under SLGA 2013"]
    for col, items in ((Inches(0.7), left), (Inches(6.9), right)):
        text(s, col, Inches(1.95), Inches(5.7), Inches(4.6),
             [(f"·  {i}", 12.5, False, INK, 9) for i in items], spacing=1.15)
    box(s, Inches(0.7), Inches(6.0), Inches(11.9), Inches(0.9), fill=PAPER, line=LINE)
    text(s, Inches(1.0), Inches(6.18), Inches(11.3), Inches(0.6),
         [("The corpus is yours to extend. Your IT staff are trained to add "
           "instruments; Noviq does not gatekeep what goes in.", 13.5, False, INK, 0)],
         spacing=1.2)
    footer(s, n)

    # ── 17 currency ─────────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "Why currency is the whole game", "A stale copy is worse than no copy.")
    text(s, Inches(0.7), Inches(1.95), Inches(11.9), Inches(0.8),
         [("An officer who knows they lack the rule will go and look it up. An "
           "officer given the superseded text confidently will not.", 16, False, INK, 0)],
         spacing=1.25)
    stat(s, Inches(0.7), Inches(3.0), Inches(3.7), "7 amendments",
         "Constitutional amendments a 2012 text is missing — including the "
         "one creating the Federal Constitutional Court.", NAVY)
    stat(s, Inches(4.8), Inches(3.0), Inches(3.7), "4 years",
         "How long a superseded procurement threshold can sit in a document "
         "library unnoticed.", NAVY)
    stat(s, Inches(8.9), Inches(3.0), Inches(3.7), "Struck down",
         "Rules exist in published copies after courts have voided them.", NAVY)
    text(s, Inches(0.7), Inches(5.0), Inches(11.9), Inches(1.6),
         [("Our commitment to you", 15, True, TEAL, 10),
          ("Every instrument carries the date it was amended to, and that date is "
           "shown with the answer. A currency review runs before each deployment. "
           "Where a court has struck down a provision we hold, that fact sits next "
           "to the text.", 14, False, INK, 0)], spacing=1.25)
    footer(s, n)

    # ── 18 data ─────────────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "Where your data lives", "On your server, in your building.")
    bullets(s, Inches(0.7), Inches(2.05), Inches(11.9), [
        "Noviq is installed on a server the department owns and controls.",
        "The corpus, the index and the query history remain on that server.",
        "Access is per-user with an administrator console and an audit log of "
        "who asked what.",
        "A user's own uploaded documents are visible only to that user unless "
        "explicitly shared.",
    ], size=15, gap=13)
    box(s, Inches(0.7), Inches(4.5), Inches(11.9), Inches(1.6),
        fill=RGBColor(0xFD, 0xF8, 0xEC), line=AMBER)
    text(s, Inches(1.0), Inches(4.75), Inches(11.3), Inches(1.2),
         [("One thing we will not gloss over", 14, True, INK, 8),
          ("Language generation can run on hardware inside the department, or on a "
           "hosted model service. Those are different privacy positions. We will "
           "state plainly which one your deployment uses, and it is your decision "
           "— not a default we set quietly.", 13.5, False, INK, 0)], spacing=1.22)
    footer(s, n)

    # ── 19 beta scope ───────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "The beta", "What is in, and what is not yet.")
    box(s, Inches(0.7), Inches(1.95), Inches(5.7), Inches(4.1), fill=WHITE, line=TEAL, lw=1.5)
    text(s, Inches(1.0), Inches(2.2), Inches(5.1), Inches(3.6),
         [("In the beta", 16, True, TEAL, 12),
          ("·  Ask any question across the 18 instruments", 13, False, INK, 9),
          ("·  Answers quote the provision and name the instrument", 13, False, INK, 9),
          ("·  Show-cause notice assistance", 13, False, INK, 9),
          ("·  Upload your own documents and ask about them", 13, False, INK, 9),
          ("·  Per-user accounts, admin console, audit log", 13, False, INK, 9),
          ("·  On-premises installation and IT handover", 13, False, INK, 0)],
         spacing=1.15)
    box(s, Inches(6.9), Inches(1.95), Inches(5.7), Inches(4.1), fill=PAPER, line=LINE)
    text(s, Inches(7.2), Inches(2.2), Inches(5.1), Inches(3.6),
         [("Not yet", 16, True, SLATE, 12),
          ("·  Assisted drafting of summaries and notings", 13, False, INK, 9),
          ("·  Instruments outside the eighteen loaded", 13, False, INK, 9),
          ("·  Federal law — this corpus is Sindh", 13, False, INK, 9),
          ("·  Case law search beyond judgments we load", 13, False, INK, 9),
          ("·  Integration with e-Dafter", 13, False, INK, 0)],
         spacing=1.15)
    text(s, Inches(0.7), Inches(6.3), Inches(11.9), Inches(0.6),
         [("We would rather you know the boundary now than discover it in month two.",
           14, True, NAVY, 0)])
    footer(s, n)

    # ── 20 continuity ───────────────────────────────────────────────────────
    # Anchored on Sindh's OWN programme, not a federal commission. Positions Noviq
    # as the second half of a direction the province has already chosen.
    n += 1; s = blank(prs)
    header(s, "Context", "Sindh has already chosen this direction.")
    box(s, Inches(0.7), Inches(2.05), Inches(11.9), Inches(1.5), fill=PAPER, line=LINE)
    text(s, Inches(1.05), Inches(2.28), Inches(11.2), Inches(1.1),
         [("The province is implementing e-Dafter to replace paper-based file "
           "movement with a digitally managed workflow — real-time processing and "
           "tracking, device-bound access, digital signatures, secure storage.",
           15, False, INK, 0)], spacing=1.25)
    text(s, Inches(0.7), Inches(3.9), Inches(11.9), Inches(2.3),
         [("That solves how the file MOVES. It does not solve what the file is ABOUT.",
           19, True, NAVY, 14),
          ("A file can arrive instantly, signed, tracked and stored — and still "
           "carry a noting that cites a rule struck down two years ago, or a "
           "threshold that changed in 2022. Digital movement makes the wrong answer "
           "travel faster.", 15, False, INK, 12),
          ("Noviq is the second half of the same programme: the officer can ask the "
           "corpus a question and get the governing provision back, cited.",
           15, True, TEAL, 0)], spacing=1.25)
    footer(s, n)

    # ── 21 deployment ───────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "How a deployment works", "Installed, handed over, supported.")
    flow(s, Inches(0.7), Inches(2.3), [
        ("1. Install", "On a departmental\nserver"),
        ("2. Load", "Your instruments,\ncurrency-checked"),
        ("3. Train", "Department IT own\nthe corpus"),
        ("4. Pilot", "One department,\nagreed period"),
        ("5. Support", "Annual support\nagreement"),
    ], w_each=Inches(2.2))
    text(s, Inches(0.7), Inches(4.0), Inches(5.7), Inches(2.4),
         [("What we do", 15, True, TEAL, 10),
          ("Install, load and verify the corpus, train your staff, and support the "
           "system under a written agreement.", 13.5, False, INK, 0)], spacing=1.2)
    text(s, Inches(6.9), Inches(4.0), Inches(5.7), Inches(2.4),
         [("What the department does", 15, True, TEAL, 10),
          ("Owns the corpus and decides what goes in. We define the support boundary "
           "in writing so responsibility is never ambiguous.", 13.5, False, INK, 0)],
         spacing=1.2)
    footer(s, n)

    # ── 22 the ask ──────────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "What we are asking for", "One department, one agreed period.")
    bullets(s, Inches(0.7), Inches(2.1), Inches(11.9), [
        "A single department willing to run Noviq alongside existing practice.",
        "A named focal officer — ideally a Deputy Secretary who sees the "
        "files, not only the system.",
        "A server, and the department's IT staff for the handover.",
        "Agreement on the instruments to load first, and who verifies their currency.",
        "Honest feedback, including when an answer is wrong. Wrong answers are the "
        "most useful thing you can send us.",
    ], size=15, gap=14)
    box(s, Inches(0.7), Inches(5.35), Inches(11.9), Inches(1.25), fill=PAPER, line=NAVY)
    text(s, Inches(1.0), Inches(5.6), Inches(11.3), Inches(0.9),
         [("How we would like to be judged: fewer repeat audit paragraphs, fewer "
           "orders set aside on procedure, and officers reaching for it without "
           "being told to.", 15, True, NAVY, 0)], spacing=1.25)
    footer(s, n)

    # ── 23 close ────────────────────────────────────────────────────────────
    n += 1; s = blank(prs); box(s, 0, 0, W, H, fill=NAVY)
    text(s, Inches(1.1), Inches(2.5), Inches(11), Inches(2.4),
         [("The rules are already correct.", 34, True, WHITE, 18),
          ("Noviq puts them in front of the officer at the moment the decision is "
           "made — with the citation attached.", 22, False,
           RGBColor(0xC8, 0xD8, 0xE4), 0)], spacing=1.25)
    box(s, Inches(1.1), Inches(5.1), Inches(2.2), Pt(3), fill=TEAL)
    text(s, Inches(1.1), Inches(5.5), Inches(11), Inches(0.6),
         [("Questions", 16, True, RGBColor(0x9F, 0xB8, 0xCC), 0)])

    # ── 24 appendix ─────────────────────────────────────────────────────────
    n += 1; s = blank(prs)
    header(s, "Appendix", "Presenter notes — not for projection")
    bullets(s, Inches(0.7), Inches(2.0), Inches(11.9), [
        "Screenshots on scenario slides MUST be captured from the live system "
        "before presenting. Do not present mock answers.",
        "Do NOT demonstrate deceased-quota questions. Rule 11-A was struck down "
        "(26 Sep 2024) and deleted by the Cabinet (04.12.2024), but published "
        "copies still carry it.",
        "Confirm whether the Sindh RSO 2000 has been repealed before presenting "
        "Scenario 1 as current law. The federal ordinance was repealed in 2010; "
        "the Sindh position is unconfirmed.",
        "No accuracy percentages are quoted anywhere, deliberately — we have "
        "not run generation-based evaluations, so any figure would be invented.",
        "If asked “does it ever get things wrong?” — answer yes, "
        "that is why every sentence carries a citation the officer can open.",
        "Detailed swimlane flows are in the companion document.",
    ], size=13.5, gap=12)
    footer(s, n)

    prs.save(path)
    return len(prs.slides.__iter__.__self__._sldIdLst)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "noviq-beta-launch.pptx"
    build(out)
    print(f"written: {out}")
