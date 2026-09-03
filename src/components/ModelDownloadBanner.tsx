/**
 * ModelDownloadBanner — shown when the LLM model file is not present.
 * Prompts the user to download it and shows live progress.
 * After download completes, prompts user to restart so the model loads.
 */
import { Component, createSignal, onMount, onCleanup, Show } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { checkModel, downloadModel, restartApp } from "../api";
import type { ModelStatus } from "../api";

interface DownloadProgressEvent {
  percent: number;
  downloaded_mb: number;
  total_mb: number;
}

const ModelDownloadBanner: Component = () => {
  const [status, setStatus] = createSignal<ModelStatus | null>(null);
  const [downloading, setDownloading] = createSignal(false);
  const [progress, setProgress] = createSignal(0);
  const [downloadedMb, setDownloadedMb] = createSignal(0);
  const [totalMb, setTotalMb] = createSignal(0);
  const [error, setError] = createSignal<string | null>(null);
  const [dismissed, setDismissed] = createSignal(false);
  const [needsRestart, setNeedsRestart] = createSignal(false);
  const [restarting, setRestarting] = createSignal(false);

  onMount(async () => {
    const unlisten = await listen<DownloadProgressEvent>(
      "model:download-progress",
      (event) => {
        const { percent, downloaded_mb, total_mb } = event.payload;
        setProgress(percent);
        setDownloadedMb(downloaded_mb);
        setTotalMb(total_mb);
        if (percent >= 100) {
          setDownloading(false);
          setNeedsRestart(true);
        }
      },
    );
    onCleanup(() => unlisten());

    try {
      const s = await checkModel();
      setStatus(s);
      // Auto-provision on first launch — no user action required.
      if (!s.exists) {
        handleDownload();
      }
    } catch {
      // Can't check model — suppress banner
    }
  });

  const handleDownload = async () => {
    setError(null);
    setDownloading(true);
    setProgress(0);
    try {
      await downloadModel();
    } catch (e) {
      setError(String(e));
      setDownloading(false);
    }
  };

  const handleRestart = async () => {
    setRestarting(true);
    try {
      await restartApp();
    } catch {
      // restartApp kills the process — error here means something went wrong
      setRestarting(false);
    }
  };

  // Don't render if model exists, user dismissed, status unknown, or restarting.
  const visible = () =>
    !dismissed() &&
    !restarting() &&
    status() !== null &&
    (!status()!.exists || needsRestart());

  return (
    <Show when={visible()}>
      <div class="fixed bottom-4 left-1/2 -translate-x-1/2 z-50 w-full max-w-md mx-4">
        <div class="bg-noviq-panel border border-noviq-line rounded-xl p-4 shadow-2xl">
          <Show when={needsRestart()}>
            <p class="text-sm font-semibold text-noviq-700">Setup complete</p>
            <p class="text-xs text-noviq-500 mt-0.5">
              Restart the app to finish.
            </p>
            <button
              class="mt-3 w-full text-sm bg-noviq-700 hover:bg-noviq-800 text-white
                     rounded-lg py-2 px-3 transition-colors font-medium"
              onClick={handleRestart}
            >
              Restart Now
            </button>
          </Show>

          <Show when={!needsRestart()}>
            <div class="flex items-center gap-2">
              <span class="h-3 w-3 rounded-full border-2 border-noviq-300 border-t-noviq-700 animate-spin" />
              <p class="text-sm font-semibold text-noviq-ink">Downloading the model…</p>
            </div>

            {/* Progress loader */}
            <div class="mt-3">
              <div class="h-2 bg-noviq-100 rounded-full overflow-hidden">
                <div
                  class="h-full bg-noviq-700 rounded-full transition-all duration-300"
                  style={{ width: `${progress()}%` }}
                />
              </div>
              <p class="text-right text-xs text-noviq-500 mt-1">{progress()}%</p>
            </div>

            <Show when={error()}>
              <p class="mt-2 text-xs text-red-600">
                Download failed. Retrying may help.
              </p>
              <button
                class="mt-2 w-full text-sm bg-noviq-700 hover:bg-noviq-800 text-white
                       rounded-lg py-2 px-3 transition-colors font-medium"
                onClick={handleDownload}
              >
                Retry
              </button>
            </Show>
          </Show>
        </div>
      </div>
    </Show>
  );
};

export default ModelDownloadBanner;
