# Evidence Dossier — Documented Cases Where the Rule Existed and Was Not Applied

Hard, citable evidence for the beta-launch deck. Compiled 2026-07-26 from court
reports, Auditor-General reports and mainstream press.

**Every case is classified by whether Noviq could actually have prevented it.**
That column is the point of this document. A room containing lawyers and serving
officers will test any overclaim immediately, and the fastest way to lose them is to
present a corruption case as something a search tool would have stopped. Being
visibly precise about what we *cannot* fix is what makes the rest credible.

| Verdict | Meaning |
|---|---|
| **YES** | A knowledge/currency gap. The officer needed to know which rule applied, in what version, at what threshold. This is exactly what Noviq does. |
| **PARTLY** | Noviq supplies the provision, but the failure also had a discretionary or interpretive element it cannot decide. |
| **NO** | Deliberate or political. The rule was known and overridden. Noviq changes nothing except leaving a record of what the rule said. |

---

## ⚠️ READ FIRST — this dossier found a defect in our own product

**Case 2 below is both our strongest demo and, today, a live liability.**

Rule 11-A of the Sindh Civil Servants (APT) Rules 1974 was struck down by the
Supreme Court in 2024 and deleted by the Sindh Cabinet on **04.12.2024**. I checked
the corpus on the production VM:

```
/opt/noviq/data/uploads/admin/11_Sindh_Civil_Servants_APT_Rules_1974.pdf
  line 656: 6[11-A.-
  line 700: Rule 11-A, substituted by Notification No. SOR-I(S&GAD)2-3/02 (Pt. III)
            dt. 30-7-2011 ...
```

**Rule 11-A is still in our corpus, carrying its 2011 substitution footnote and no
record of its deletion or of the judgment striking it down.** So if an officer asks
Noviq today whether they can appoint on deceased quota under Rule 11-A, Noviq will
cite a dead provision, confidently, with a citation that looks authoritative.

That is precisely the failure we are selling against. Two consequences:

1. **Do not demo anything touching Rule 11-A or deceased quota until the corpus is
   updated.** A serving officer in that room may well know about the December 2024
   deletion — several will have processed the fallout.
2. **This is a beta-blocking fix, not a follow-up.** It also generalises: we should
   assume other instruments are similarly stale until checked. §5 proposes the check.

Presenting "currency of law" as a strength while our own copy is nine months stale is
the single biggest credibility risk in this launch.

---

## 1. Wrong instrument applied — entire proceedings vitiated

**What happened.** Sindh departments repeatedly conducted complete disciplinary
processes — charge sheet, departmental inquiry, penalty order, final order — under
the **Sindh Civil Servants (E&D) Rules 1973** when the **Removal from Service
(Special Powers) Sindh Ordinance 2000** applied, or the reverse.

**Consequence.** The Sindh High Court held that where action was initiated and
carried to its conclusion *"under a misconception of law and under a wrong law"*, it
**vitiated the entire proceedings** including the final order, which could not be
sustained. The department must start again — and the officer is meanwhile reinstated.

**Why it is a knowledge gap.** Two overlapping instruments govern the same subject
with different procedures and different timelines. Choosing between them is a legal
question asked at the very start of a case, by a Section Officer, usually without
counsel.

**Noviq: YES.** Both instruments are in the corpus. This is a "which of these two
governs, and what does each require" question — the core competence.

*Sources: High Court of Sindh caselaw portal; practitioner commentary on E&D remedies.*

---

## 2. A rule struck down and deleted — still being applied

**What happened.** In **General Post Office, Islamabad v. Muhammad Jalal, PLD 2024
SC 1276** (Civil Petition No. 3390 of 2021) the Supreme Court declared **Rule 11-A of
the Sindh Civil Servants (APT) Rules 1974** — the deceased/son quota appointment
provision — **ultra vires and unconstitutional**. The Sindh Cabinet deleted the rule
on **04.12.2024**.

**The complication that makes this a genuine trap.** The judgment operates
**prospectively**. In **Zahida Parveen v. Government of Khyber Pakhtunkhwa (2025 SCP
107)** the Supreme Court clarified that PLD 2024 SC 1276 *"does not take away the
accrued rights and/or set aside the past and closed transactions."* So an officer
must know three things at once: the rule is gone; the judgment is prospective only;
and rights that accrued before it survive.

**Consequence.** Litigation on both sides — departments wrongly refusing accrued
claims, and departments wrongly granting new ones. As late as **10 March 2026** the
Federal Constitutional Court rejected a Sindh government petition against an
appointment on deceased quota.

**Noviq: YES in principle — NO today.** See the warning above. This becomes the
single best demo in the deck *only after* the corpus is corrected, because it
combines repeal, a dated cabinet decision, and a prospective-effect nuance that no
officer can be expected to hold in their head.

---

## 3. The law changed three weeks ago — Sindh, promotions

**What happened.** On **3 July 2026** a three-member Supreme Court bench (Justice
Muhammad Ali Mazhar, Justice Aqeel Ahmed Abbasi, Justice Muhammad Shafi Siddiqui),
deciding two civil appeals against a **Sindh Service Tribunal** decision, held that
**contractual service cannot be counted toward the qualifying service required for
promotion**. Contract employees fall outside the statutory definition of "civil
servant" under the **Sindh Civil Servants Act 1973**.

**The sting.** The Court held that *"an unlawful benefit mistakenly extended to one
employee in the past did not create a legal right for others to claim the same
concession."* Past erroneous promotions are no precedent — so "we have always done it
this way" is now an expressly rejected defence.

**Consequence.** Any Sindh promotion case currently counting contract service is
being processed wrongly, three weeks after the law changed.

**Noviq: YES — and this is the most current, most urgent example available.** It is
also the cleanest argument for the product: no officer can track Supreme Court
judgments in real time across every service rule. A system can.

*Source: The Nation, 3 July 2026.*

---

## 4. Recruitment body itself declared unlawful

**What happened.** The Hyderabad circuit bench of the Sindh High Court (Justice
Zulfiqar Ahmed Khan and Justice Muhammad Saleem Jessar) declared the **Sindh Public
Service Commission defunct**, holding the **SPSC Act 1989 ultra vires the
Constitution**, and the rules for appointing the chairperson and members **ultra
vires the Sindh Government Rules of Business 1986**.

**Consequence.** The court ordered that *"all tests, interviews, selection,
appointments, tenders or any act doable under SPSC Act, 1989, or the rules are
suspended forthwith"*, and declared appointments under **CCE 2018** and the Medical
Officers' examinations **null and void** — after 30+ years of the commission
recruiting for the province. Separately, in 2026 the appointment of an SPSC chairman
was declared unlawful. NAB had opened an inquiry into illegal SPSC appointments in
May 2020.

**Noviq: PARTLY.** The Rules of Business 1986 ground of invalidity is squarely in our
corpus, so the conflict between an appointment rule and RoB 1986 is the kind of
inconsistency Noviq can surface. But whether a statute is ultra vires the
Constitution is a judicial determination, not a retrieval result. **Say so.**

*Sources: The Express Tribune; Global Village Space; Dawn.*

---

## 5. Audit paras that repeat "year after year" — the procurement threshold

**What happened.** **Rule 17(1), Sindh Public Procurement Rules 2010:** procurements
over **Rs 100,000 and up to Rs 1 million** must be advertised by timely notification
on the Authority's website, and may additionally be in print media. Departments
repeatedly awarded work **without inviting open tenders or advertising at all**.

**Consequence (Sindh, AGP audit of Union Committees/Councils).** *"Violation of
Rules"* in **14 cases involving Rs 194.884 million**; pre-audit *"almost
non-functional"*; and the finding that matters most —

> *"similar nature of internal control violations reported year after year."*

**Federal scale, for context (AGP 2025–26 audit of civil accounts, 399 pages).**
**Rs 3,177 billion** in unapproved supplementary grants, ~92% bypassing
parliamentary approval. Ministry of Interior & Narcotics Control topped the list with
**65 audit paras**; HEC 31; TDAP 18. Health showed **Rs 3.41 billion** in
irregularities, **Rs 1,779.26 million** of it procurement-related. Rs 1.2 billion was
spent overhauling two helicopters **without open competitive bidding**.

**Noviq: PARTLY — and the split is the honest point.** A precise numeric threshold
that is breached every year is not a contested rule; the knowledge simply is not
reaching the officer at the moment of decision, and Noviq answers exactly that. But
some of these are deliberate. **Do not claim Noviq prevents procurement fraud.**
Claim it removes "I did not know the threshold" as an explanation — which is
itself valuable, because it narrows every future audit para to a deliberate act.

*Sources: AGP audit report on Union Committees/Councils of Sindh; Dawn; Bloom Pakistan.*

---

## 6. Hard procedural deadlines nobody remembers

**What happened.** Under the **Removal from Service (Special Powers) Sindh Ordinance
2000**, an inquiry **must be completed within 25 days** of initiation. Under the E&D
Rules the sequence is mandatory: call for explanation → serve show-cause → appoint
inquiry officer or committee.

**Consequence.** Failure to comply fully with the rules *"impairs and at times
violates"* the proceedings, and the penalty order is **frequently set aside** by the
appellate authority or the Service Tribunal on challenge.

**Noviq: YES.** A hard, checkable, numeric deadline attached to a specific instrument
is the easiest possible win — and one of the two candidate instruments carries it
while the other does not, compounding Case 1.

---

## 7. Defective show-cause notices — natural justice

**What happened.** A show-cause notice must state the charges and the specific
grounds on which action is proposed. The documented common failure is a notice too
vague for the accused to prepare a defence.

**Consequence.** The Tribunal treats a defective notice as violating natural justice
and **vitiating the entire process**. Penalties are quashed; reinstatement with back
benefits follows.

**Noviq: YES — and already verified.** Notice Assist is user-verified working on
exactly this task. This is our safest live demonstration.

---

## 8. Interpretation, not ignorance — deceased quota deadline

**What happened.** The Sindh government read Rule 11-A as requiring application
within two years of a civil servant's death or incapacity, and rejected applications
outside it — including from children who had been minors at the time and applied on
reaching majority.

**Consequence.** In **Province of Sindh through Chief Secretary v. Azhar Ali**
(reported 2025 SCLR 6, decided 1 September 2025) the Supreme Court upheld the High
Court's grant of relief, holding that beneficial provisions must be read *"with
generous approach and reading it with widest conceivable denotation to protect the
interests of the beneficiaries."* Appeals dismissed, leave refused. The government
lost, having litigated to the apex court.

**Noviq: PARTLY.** Noviq can surface the provision and the governing judgment, which
is more than the department had. It cannot decide whether an interpretation is
liberal enough. *Citation note: the reporting year appears as both 2025 and 2026
across sources — verify the citation before it goes on a slide.*

---

## 9. Where Noviq does NOT help — say this out loud

**OPS ("Own Pay Scale") and additional-charge postings.** The Supreme Court ordered
that junior officers not be posted to senior slots under cover of OPS. The Sindh High
Court held OPS postings and additional charge a **violation of law** and directed the
Chief Secretary to avoid them. The practice continued. Press reporting describes
**selective compliance** — officers not shown as OPS in posting orders, and hurried
promotions to defeat the ruling. Reported scale: **110 officers in Grade 17 and
above**, and **5,000 below Grade 16**, given out-of-turn promotions or absorbed from
other departments.

**Noviq: NO.** The rule was known. It was overridden as a matter of policy and
patronage. No retrieval system changes that.

**Include this slide anyway.** Stating plainly that Noviq does not solve political
override does three things: it proves we understand the environment, it inoculates
against the "so it will not change anything" objection, and it lets us make the
narrower claim that *does* hold — that once the rule and its citation are on the file,
a deviation becomes a visible, documented choice rather than a plausible oversight.

*Sources: Dawn; Pakistan Today; Sindh High Court reporting.*

---

## 10. Recommended actions before the deck is built

1. **Fix the corpus currency problem.** At minimum: re-ingest the APT Rules 1974 in
   its post-04.12.2024 form, and add the Supreme Court judgments in Cases 2 and 3.
   Until then those scenarios are off-limits for demonstration.
2. **Run a currency audit across all twelve instruments.** For each, establish the
   amendment date of our copy versus the latest available. The Rule 11-A finding was
   luck; the others are unknown. This is a half-day of work and it protects the launch.
3. **Verify every demo question against the live system**, checking the answer body
   rather than the HTTP status — refusals return 200, so a status check proves nothing.
4. **Confirm the Azhar Ali citation year** before it appears on a slide.
5. **Build the "what Noviq does not do" slide** from §9. It is the credibility anchor
   for everything else.

---

## 11. Sources

1. High Court of Sindh caselaw portal · https://caselaw.shc.gov.pk/
2. Misconduct inquiries under E&D Rules — tribunal remedies · https://nmklegal.com/misconduct-inquiries-under-ed-rules-remedies-in-tribunals/
3. Removal from Service (Special Powers) Sindh Ordinance 2000 · https://sstsindh.gov.pk/admin/upload/rules/REMOVAL%20FROM%20SERVICE%20(SPECIAL%20POWERS)%20SINDH%20ORDINANCE,%202000.pdf
4. Sindh Civil Servants (E&D) Rules 1973 · https://sindhhighcourt.gov.pk/laws/SINDH%20CIVIL%20SERVANTS%20(EFFICIENCY%20&%20DISCIPLINE)%20RULES,%201973.pdf
5. Sindh Civil Servants (APT) Rules 1974 · https://sindhhighcourt.gov.pk/laws/SINDH%20CIVIL%20SERVANTS%20(APPOINTMENT,%20PROMOTION%20AND%20TRANSFER)%20RULES,%201974.pdf
6. Province of Sindh v. Azhar Ali (SCLR) · https://pakistankanoon.com/sclr2026.php/2026-SCLR-6
7. Zahida Parveen v. Govt of KP, 2025 SCP 107 · https://sahsol.lums.edu.pk/node/24186
8. The Nation — contract service cannot be counted towards promotion (3 Jul 2026) · https://www.nation.com.pk/03-Jul-2026/contract-service-cannot-counted-towards-promotion-rules-sc
9. The Nation — FCC rejects Sindh govt petition, deceased quota (10 Mar 2026) · https://www.nation.com.pk/10-Mar-2026/fcc-rejects-sindh-govt-petition-appointment-deceased-quota
10. Express Tribune — SPSC defunct, rules court · https://tribune.com.pk/story/2303403/sindh-public-service-commission-defunct-rules-court
11. Global Village Space — SHC suspends SPSC, appointments null and void · https://www.globalvillagespace.com/shc-suspends-sindh-public-service-commission-declares-appointments-exams-null-void/
12. Dawn — appointment of SPSC chairman declared unlawful · https://www.dawn.com/news/2011723
13. Dawn — SHC bars Sindh govt from appointing officers on OPS basis · https://www.dawn.com/news/1675643
14. Dawn — Sindh govt's 'selective' compliance with SC ruling on OPS · https://www.dawn.com/news/1110051
15. Sindh Public Procurement Rules 2010 (rev. 2013) · https://pppunitsindh.gov.pk/wp-content/uploads/2024/07/Sindh-Public-Procurement-Rules-2010-Revised-2013.pdf
16. AGP — audit of Union Committees/Councils of Sindh · https://agp.gov.pk/SiteImage/Policy/FINAL-UC-2017-18.pdf
17. Dawn — Interior ministry tops AGP audit irregularities · https://www.dawn.com/news/2011270/interior-ministry-tops-agp-audit-irregularities
18. Dawn — Rs3.41bn irregularities in health sector · https://www.dawn.com/news/2010834
19. Bloom Pakistan — Rs 3,177bn unapproved spending · https://bloompakistan.com/audit-report-reveals-massive-rs-3-177-trillion-unapproved-spending-in-pakistan/
