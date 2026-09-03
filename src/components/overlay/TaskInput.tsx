/**
 * TaskInput — T019.
 * Textarea + submit for entering agent task instructions.
 */
import { Component, createSignal } from "solid-js";

const MAX_CHARS = 2000;

interface TaskInputProps {
  onSubmit: (instruction: string) => void;
}

const TaskInput: Component<TaskInputProps> = (props) => {
  const [text, setText] = createSignal("");

  const submit = () => {
    const trimmed = text().trim();
    if (!trimmed) return;
    props.onSubmit(trimmed);
    setText("");
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && e.ctrlKey) {
      e.preventDefault();
      submit();
    }
  };

  return (
    <div class="p-3 border-b border-gray-700/50">
      <textarea
        class="w-full bg-gray-800 text-gray-100 text-sm rounded-lg px-3 py-2 resize-none border border-gray-600 focus:border-blue-500 focus:outline-none placeholder-gray-500"
        placeholder="Describe a task for Noviq…"
        rows={3}
        maxLength={MAX_CHARS}
        value={text()}
        onInput={(e) => setText(e.currentTarget.value)}
        onKeyDown={handleKeyDown}
      />
      <div class="flex items-center justify-end mt-1.5">
        <span class="text-xs text-gray-500">{text().length}/{MAX_CHARS}</span>
        <button
          class="ml-2 px-4 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded-lg disabled:opacity-40 disabled:cursor-not-allowed"
          disabled={text().trim().length === 0}
          onClick={submit}
        >
          Submit
        </button>
      </div>
    </div>
  );
};

export default TaskInput;
