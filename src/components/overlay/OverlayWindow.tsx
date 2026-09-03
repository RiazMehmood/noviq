/**
 * OverlayWindow — T019.
 * Floating panel container for the agent overlay window.
 */
import { Component, JSX } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

interface OverlayWindowProps {
  children: JSX.Element;
}

const OverlayWindow: Component<OverlayWindowProps> = (props) => {
  const handleMinimize = () => {
    invoke("agent_hide_overlay").catch(console.error);
  };

  return (
    <div class="fixed inset-0 flex items-start justify-center pt-16 pointer-events-none">
      <div class="w-[480px] max-h-[640px] bg-gray-900/95 backdrop-blur-sm border border-gray-700 rounded-xl shadow-2xl flex flex-col pointer-events-auto">
        {/* Drag handle */}
        <div class="h-8 flex items-center justify-between px-3 bg-gray-800/80 rounded-t-xl cursor-grab select-none flex-shrink-0">
          <span class="text-xs text-gray-400">Noviq Agent</span>
          <button
            class="text-gray-500 hover:text-gray-200 text-lg leading-none"
            onClick={handleMinimize}
          >
            −
          </button>
        </div>

        {/* Content slot */}
        <div class="flex-1 overflow-y-auto">
          {props.children}
        </div>

        {/* Resize handle */}
        <div class="h-1.5 bg-gray-700/50 rounded-b-xl cursor-s-resize flex-shrink-0" />
      </div>
    </div>
  );
};

export default OverlayWindow;
