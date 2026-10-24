import { useState } from "preact/hooks";

export interface TagInputProps {
  tags: string[];
  label: string;
  placeholder: string;
  removeLabel: (tag: string) => string;
  onChange: (tags: string[]) => void;
}

/** Tags as chips: Enter or a comma adds what was typed, × or Backspace on an empty field removes. */
export function TagInput({ tags, label, placeholder, removeLabel, onChange }: TagInputProps) {
  const [typed, setTyped] = useState("");

  const add = (text: string) => {
    const tag = text.trim();
    setTyped("");
    if (tag !== "" && !tags.some((t) => t.toLowerCase() === tag.toLowerCase())) onChange([...tags, tag]);
  };

  return (
    <div class="tag-input">
      {tags.map((tag) => (
        <span key={tag} class="tag">
          {tag}
          <button type="button" aria-label={removeLabel(tag)} onClick={() => onChange(tags.filter((t) => t !== tag))}>
            ×
          </button>
        </span>
      ))}
      <input
        value={typed}
        aria-label={label}
        placeholder={tags.length === 0 ? placeholder : ""}
        maxLength={30}
        onInput={(e) => {
          const value = (e.target as HTMLInputElement).value;
          if (/[,，]/u.test(value)) add(value.replace(/[,，]/gu, ""));
          else setTyped(value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            add(typed);
          } else if (e.key === "Backspace" && typed === "" && tags.length > 0) {
            onChange(tags.slice(0, -1));
          }
        }}
        onBlur={() => add(typed)}
      />
    </div>
  );
}
