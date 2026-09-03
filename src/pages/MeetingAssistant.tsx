/**
 * Meeting Assistant — live meeting capture & minutes (Coming soon).
 *
 * The shipped feature will listen to a meeting, transcribe it locally, and
 * draft action points and minutes grounded in what was actually said.
 */
import { Component } from "solid-js";
import ComingSoon, { FlowStep } from "../components/ComingSoon";

const flow: FlowStep[] = [
  { actor: "Capture", detail: "Records the meeting locally — nothing leaves your machine." },
  { actor: "Transcribe", detail: "Converts speech to text on-device, speaker by speaker." },
  { actor: "Summarise", detail: "Drafts decisions, action points and assignees from the transcript." },
  { actor: "Export", detail: "Produces ready-to-circulate minutes you can review and send." },
];

const MeetingAssistant: Component = () => {
  return (
    <ComingSoon
      title="Meeting Assistant"
      subtitle="Local meeting capture, transcription & minutes"
      description={
        "Run Noviq during a meeting and it will transcribe the discussion " +
        "entirely on your device, then draft the minutes — decisions, action " +
        "points and who is responsible — grounded in what was actually said. " +
        "Fully offline, nothing is uploaded."
      }
      flowCaption="How it will work"
      flow={flow}
    />
  );
};

export default MeetingAssistant;
