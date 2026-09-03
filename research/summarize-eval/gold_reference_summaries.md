# Gold Reference Summaries — for critically analysing Noviq's summary generation

Each reference is layered from **(1) my own knowledge**, **(2) web-verified authoritative sources**,
and **(3) your indexed corpus** (the provisions actually in `echo.db`). Use these to grade Noviq's
ingest-time `document_full` summaries.

**How to critique Noviq's summary against a gold entry:**
- **Coverage** — did it hit the "MUST-hit areas"?
- **Accuracy** — do its specifics match the *verified facts* (numbers, forums, timelines)? Any
  contradiction = hallucination.
- **Fabrication check** — does it assert specifics NOT in the doc/verified facts? (e.g. inventing a
  penalty scale, a wrong appeal forum, a wrong day-count).
- **Fidelity to source** — Noviq only sees the indexed text, so it should NOT know external facts
  like "later repealed." If it states one, that's a fabrication; if it omits it, that's expected
  (I flag those as *gold-only caveats*, not gradable gaps).
- **Cleanliness** — no leaked scratch reasoning, no echoed prompt ("WHAT THE DOCUMENT GOVERNS:"),
  no section numbers if the spec says none.

---

## G1 — Sindh Public Procurement (SPPRA) Rules, 2010 (Revised 2013)  *(doc_type: rules)*

**Summary.** Made under the Sindh Public Procurement Act 2009 and administered by the Sindh Public
Procurement Regulatory Authority (SPPRA), these Rules standardise how every procuring agency in Sindh
buys goods, works, and services. They codify the principles of public procurement — economy,
efficiency, transparency, fairness, and value for money — and define the key actors (bidder,
supplier, contractor, consultant, competent authority) and what counts as corrupt or fraudulent
practice. They then regulate the full cycle: prior approval to procure; the method of bidding
(open/competitive, restricted, negotiated) and thresholds; the minimum time bidders must be given
(distinguishing national from international competitive bidding); pre-qualification and eligibility;
contents of bidding documents; bid submission, opening, evaluation, and award. Financial integrity is
secured through **bid security** (for contracts above a value threshold) and **performance
security**. Accountability runs through mandatory record-keeping, a two-stage grievance route
(agency **complaints-redressal committee → appeal to a Review Committee/Panel**, conditional on the
bidder not having withdrawn bid security), and **blacklisting** of suppliers/contractors/consultants
who breach contracts or fail to perform. A dedicated part governs **selection of consultants**
(methods, quality evaluation, professional liability). The Rules assert precedence over other
government procurement instructions.

**Verified facts Noviq should be consistent with (web):**
- Grievance timeline (Revised 2013): redressal committee announces its decision within **7 days**;
  intimated to bidder/Authority within **3 working days**; Review Panel convenes within **5 working
  days**. *(Noviq may not surface exact day-counts from a whole-doc summary — fine — but it must not
  state DIFFERENT numbers.)*
- Blacklisting is a real, named mechanism; a blacklisted bidder's bid is rejected.

**MUST-hit areas:** procurement principles + scope; SPPRA/Act-2009 basis; bidding methods + response
time (NCB vs ICB); pre-qualification/eligibility; bid & performance security; evaluation/award;
record-keeping; complaints → Review Committee appeal; blacklisting; consultants selection; overriding
effect.

**Sources:** [SPPRA Rules 2010 (rev. 2013), PPP Unit Sindh](https://pppunitsindh.gov.pk/wp-content/uploads/2024/07/Sindh-Public-Procurement-Rules-2010-Revised-2013.pdf) · [PPRA Sindh downloads](https://www.pprasindh.gov.pk/downloads/spprarules2010.php) · [Blacklisting guidelines, PPRA Sindh](https://www.pprasindh.gov.pk/blacklist/Blacklistingguidelines&notification.pdf)

---

## G2 — Removal from Service (Special Powers) Sindh Ordinance, 2000  *(doc_type: act)*

**Summary.** This Ordinance (extending to the whole of Sindh) gives the competent authority special
powers to **dismiss, remove, compulsorily retire, or reduce in rank/pay** persons in government and
corporate service on grounds including misconduct, corruption, inefficiency, and being subversive or
otherwise unfit to continue (the core power is in **Section 3**). It prescribes the procedure: the
competent authority may appoint an **Inquiry Officer or Inquiry Committee**, which regulates its own
procedure — fixing time/place of sittings, deciding public or private hearing, summoning evidence —
and reports findings, on which the competent authority passes final orders. Pending action, a person
may be placed under **suspension** with immediate effect or required to proceed on admissible leave.
The Ordinance protects **representation and appeal** rights, addresses continuation of pending
proceedings irrespective of service status, pension/benefit consequences, the Government's
rule-making power, and asserts an **overriding effect** over conflicting laws.

**Verified facts Noviq should be consistent with (web + indexed text):**
- **Representation (Section 9):** an aggrieved person may, **within 15 days** of communication of the
  order, prefer a representation to the **Governor** (or a designated officer/authority).
- **Appeal (Section 10):** appeal to the **Sindh Service Tribunal within 30 days** of a final order
  under §9. *(This is the indexed verbatim — the strongest gradable fact.)*
- **Gold-only caveat (NOT expected in Noviq's summary):** the Ordinance was subsequently
  **repealed/abolished** (the federal RSO 2000 was repealed by Act III of 2010; Sindh moved to
  abolish its version). Noviq sees only the Ordinance text, so omission is correct; if Noviq *asserts*
  it is repealed, that's a fabrication.

**MUST-hit areas:** special removal powers + grounds (§3); competent authority; inquiry
officer/committee procedure; suspension/leave; representation (Governor) + appeal (Service Tribunal);
overriding effect; rule-making. **Watch:** do NOT invent a graded minor/major penalty menu — that's
the E&D Rules, not this Ordinance.

**Sources:** [RSO 2000, Sindh Service Tribunal](https://sstsindh.gov.pk/admin/upload/rules/REMOVAL%20FROM%20SERVICE%20(SPECIAL%20POWERS)%20SINDH%20ORDINANCE,%202000.pdf) · [Sindh Govt gazette copy](http://www.pas.gov.pk/uploads/downloads/The%20Removal%20from%20Service%20(Special%20Powers)%20Sindh%20Ordinance%20No.IX%20of%202000.pdf) · [Repeal note, Establishment Division](https://establishment.gov.pk/userfiles1/file/Repeal%20of%20Removal%20from%20Service%20(Special%20Powers)%20Ordinance%202000.pdf)

---

## G3 — Sindh Civil Servants (Efficiency & Discipline) Rules, 1973  *(doc_type: rules)*

**Summary.** These Rules govern disciplinary action against Sindh civil servants found **inefficient,
guilty of misconduct, corrupt, or engaged in subversive activities** (grounds in Rule 3). They set
out the penalties that may be imposed, split into **minor** and **major** penalties — censure,
withholding of increments/promotion, recovery/stoppage of pay, reduction to a lower post or pay
stage, compulsory retirement, removal from service, and dismissal (Rule 4). The procedure (Rule 5)
requires the **authorised officer** to frame a charge and statement of allegations and give the
accused an opportunity to reply; pending the case the accused may be sent on leave or **suspended**
(continued leave/suspension needing the authority's approval every three months). The authorised
officer decides whether to hold an **inquiry** through an **Inquiry Officer or Inquiry Committee**
(with civil-court powers to summon witnesses and take evidence); on the report, the authorised
officer imposes a minor penalty himself, or forwards the case to the **authority** for a major
penalty. The Rules cover exceptions to the normal inquiry (e.g. conviction on a criminal charge),
handling of deputationists, and rights of **representation/appeal** (including to the Chief Minister).

**Verified facts Noviq should be consistent with (web):**
- Penalty list explicitly includes censure, withholding promotion, stoppage of pay, demotion,
  compulsory retirement, removal, dismissal — grouped minor vs major.
- Rule 5 split of authority: **minor penalty → authorised officer; major penalty → forwarded to the
  authority**; leave/suspension re-approved every **3 months**.

**MUST-hit areas:** grounds (Rule 3); minor vs major penalties (Rule 4); charge + statement of
allegations; suspension/leave (3-month re-approval); Inquiry Officer/Committee with civil-court
powers; minor-vs-major decision authority; exceptions (conviction); representation/appeal (CM).

**Sources:** [E&D Rules 1973, Sindh High Court](https://sindhhighcourt.gov.pk/laws/SINDH%20CIVIL%20SERVANTS%20(EFFICIENCY%20&%20DISCIPLINE)%20RULES,%201973.pdf) · [24Justice summary](https://24justice.pk/the-sindh-civil-servants-efficiency-and-discipline-rules-1973/)

---

## G4 — Constitution of the Islamic Republic of Pakistan  *(doc_type: act; 12 Parts, 283 articles indexed)*

**Summary.** The supreme law of Pakistan, establishing it as a **federal parliamentary democratic
republic** with the Constitution as supreme and sovereignty resting with Almighty Allah (Objectives
Resolution). **Part I** lays foundational articles (supremacy, definition of the State, law
inconsistent with rights void). **Part II** guarantees **Fundamental Rights** (security of person;
safeguards on arrest/detention; freedoms of movement, assembly, association, trade, speech,
religion; equality before law and non-discrimination) and sets the **Principles of Policy**.
**Part III** structures the **Federation** — the President; **Parliament (Majlis-e-Shoora)** comprising
the **National Assembly and the Senate**, their composition, and the legislative/money-bill
procedure; the Federal Executive (Prime Minister and Cabinet). **Part IV** governs the **Provinces**
(Governor, Chief Minister, Provincial Assemblies), mirroring the federal structure, and the
federal–provincial **distribution of legislative and financial powers**. **Part VII** establishes the
**Judicature** — Supreme Court, High Courts, and the Federal Shariat Court — and judicial
independence. Further Parts cover **Islamic provisions** (conformity of laws with the Injunctions of
Islam; Council of Islamic Ideology), **finance/property/contracts** and audit (Auditor-General),
**elections** and the Election Commission, **emergency provisions**, **services of Pakistan**, and the
**procedure to amend the Constitution**. Many articles carry a documented history of **substitution
and insertion by successive amendments** (e.g. 8th and 18th Amendments).

**Verified facts / structure (well-established + indexed):** bicameral Parliament (NA + Senate);
three superior courts named above; 18th Amendment devolution to provinces; Article 6 (high treason)
for abrogating the Constitution; Fundamental Rights are justiciable while Principles of Policy are
not. *(A whole-doc summary need not list article numbers, but must not misstate the organs.)*

**MUST-hit areas:** supremacy + federal parliamentary character; Fundamental Rights (+ Principles of
Policy); Parliament (NA + Senate) & legislative procedure; Federal Executive; provinces + federal–
provincial power distribution; Judicature (SC/HC/FSC); Islamic provisions; finance/elections/
emergency; amendment procedure. **Expected:** 2–3 ordered segments (builder reduces ~283 article
gists in ~3 batches) — check segments read in Part order and none are dropped.

**Sources:** [Constitution of Pakistan, National Assembly](https://na.gov.pk/uploads/documents/1333523681_951.pdf) · [Pakistan Code](https://pakistancode.gov.pk/)

---

## G5–G7 — APT Rules 1974 · Delegation of Financial Powers 2019 · Rules of Business 1986
Reference summaries written from knowledge + indexed corpus in **`teacher_document_summaries.md`**
(R4/R5/R6). Web verification adds little beyond confirming the schema; grade against those + the
MUST-hit lists there.

---

## Critical-analysis scorecard (fill after running Noviq)
| Doc | Coverage (MUST-hit) | Accuracy vs verified facts | Fabrication? | Clean (no leak/echo)? | Segments correct? | Grade |
|-----|--------------------|----------------------------|--------------|-----------------------|-------------------|-------|
| G1 SPPRA | | | | | | |
| G2 RSO 2000 | | | | | | |
| G3 E&D 1973 | | | | | | |
| G4 Constitution | | | | | n/1–3 | |
| R4 APT | | | | | | |
| R5 Delegation | | | | | | |
| R6 Rules of Business | | | | | | |
