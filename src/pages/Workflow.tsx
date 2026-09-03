/**
 * Workflow — document-routing / "file movement" guide (Coming soon).
 *
 * Replaces the old Knowledge Graph tab. The shipped feature will map how a
 * given application or case moves between offices — where to initiate it, who
 * it is submitted to, every desk it passes through, and how it returns to the
 * originating officer — so a user knows the exact path to get something done.
 */
import { Component } from "solid-js";
import ComingSoon, { FlowStep } from "../components/ComingSoon";

const leaveExample: FlowStep[] = [
  { actor: "Applicant Officer", detail: "Initiates the leave application (with grounds and dates) and forwards it up." },
  { actor: "Section Officer / Dealing Assistant", detail: "Checks leave balance and service record, prepares the noting." },
  { actor: "Deputy Secretary", detail: "Examines admissibility under the leave rules and recommends." },
  { actor: "Secretary", detail: "Competent authority sanctions or rejects the leave." },
  { actor: "Back to the Applicant Officer", detail: "Sanctioned order is conveyed and the file returns to the originator." },
];

const Workflow: Component = () => {
  return (
    <ComingSoon
      title="Workflow"
      subtitle="Document routing & file-movement guide"
      description={
        "Tell Noviq what you want to get done — for example, applying to the " +
        "Secretary for 30 days' leave, or getting your previous service counted — " +
        "and it will map the exact path your application must travel: where to " +
        "initiate it, whom to submit it to, every desk it moves through, and how it " +
        "finally returns to you. No more guessing which office comes next."
      }
      flowCaption="Example — 30 days' leave application to the Secretary"
      flow={leaveExample}
    />
  );
};

export default Workflow;
