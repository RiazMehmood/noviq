import { createSignal } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

export type WorkloadKind =
  | "chat"
  | "indexing"
  | "pass2_ner"
  | "kg_extraction"
  | "file_watcher";

export interface Progress {
  processed: number;
  total: number;
  started_at_ms: number;
  last_tick_at_ms: number;
  paused: boolean;
}

export interface WorkloadState {
  id: string;
  kind: WorkloadKind;
  progress: Progress;
}

const [workloads, setWorkloads] = createSignal<Record<string, WorkloadState>>({});

type SchedulerEvent =
  | { type: "foreground_changed"; kind: WorkloadKind }
  | { type: "workload_registered"; id: string; kind: WorkloadKind }
  | { type: "workload_finished"; id: string; kind: WorkloadKind }
  | { type: "progress_tick"; id: string; kind: WorkloadKind; progress: Progress };

listen<SchedulerEvent>("workload://state", (e) => {
  const ev = e.payload;
  setWorkloads((ws) => {
    const next = { ...ws };
    switch (ev.type) {
      case "workload_registered":
        next[ev.id] = {
          id: ev.id,
          kind: ev.kind,
          progress: {
            processed: 0,
            total: 0,
            started_at_ms: Date.now(),
            last_tick_at_ms: Date.now(),
            paused: false,
          },
        };
        break;
      case "workload_finished":
        delete next[ev.id];
        break;
      case "progress_tick":
        if (next[ev.id]) {
          next[ev.id] = { ...next[ev.id], progress: ev.progress };
        }
        break;
    }
    return next;
  });
});

export function useWorkloads() {
  return workloads;
}

export function workloadsOfKind(kind: WorkloadKind) {
  return () => Object.values(workloads()).filter((w) => w.kind === kind);
}

export async function setForeground(kind: WorkloadKind) {
  await invoke("set_foreground", { kind });
}

export async function cancelIndexJob(jobId: string, workloadId: string) {
  await invoke("cancel_index_job", { jobId, workloadId });
}

export async function discardIndexJob(jobId: string) {
  await invoke("discard_index_job", { jobId });
}
