import { useEffect, useRef } from "react";
import { basicSetup } from "codemirror";
import { indentWithTab } from "@codemirror/commands";
import { EditorState, StateEffect, type Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { languageForFile } from "../languages";
import type { RevealTarget } from "../types";
import "./CodeEditor.css";

type Props = {
  path: string;
  name: string;
  value: string;
  reveal?: RevealTarget | null;
  onChange: (value: string) => void;
};

export function CodeEditor({ path, name, value, reveal, onChange }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const valueRef = useRef(value);
  const onChangeRef = useRef(onChange);
  valueRef.current = value;
  onChangeRef.current = onChange;

  useEffect(() => {
    if (!host.current) return;
    let disposed = false;

    const extensions: Extension[] = [
      basicSetup,
      keymap.of([indentWithTab]),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString());
        }
      }),
    ];

    const editor = new EditorView({
      state: EditorState.create({ doc: valueRef.current, extensions }),
      parent: host.current,
    });
    view.current = editor;

    const registration = languageForFile(name);
    if (registration) {
      void registration.load().then((language) => {
        if (disposed || !view.current) return;
        view.current.dispatch({ effects: StateEffect.appendConfig.of(language) });
      });
    }

    return () => {
      disposed = true;
      editor.destroy();
      view.current = null;
    };
  }, [path, name]);

  useEffect(() => {
    const editor = view.current;
    if (!editor || editor.hasFocus) return;
    if (editor.state.doc.toString() !== value) {
      editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: value } });
    }
  }, [value]);

  useEffect(() => {
    const editor = view.current;
    if (!editor || !reveal || reveal.path !== path) return;
    const lineNumber = Math.min(Math.max(reveal.line, 1), editor.state.doc.lines);
    const line = editor.state.doc.line(lineNumber);
    editor.dispatch({
      selection: { anchor: line.from, head: line.to },
      effects: EditorView.scrollIntoView(line.from, { y: "center" }),
    });
    editor.focus();
  }, [reveal, path]);

  return <div className="editor-host" ref={host} />;
}
