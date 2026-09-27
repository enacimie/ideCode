import type { Extension } from "@codemirror/state";

export type LanguageRegistration = {
  id: string;
  label: string;
  extensions: string[];
  load: () => Promise<Extension>;
};

export const languages: LanguageRegistration[] = [
  {
    id: "java",
    label: "Java",
    extensions: ["java"],
    load: async () => (await import("@codemirror/lang-java")).java(),
  },
  {
    id: "python",
    label: "Python",
    extensions: ["py"],
    load: async () => (await import("@codemirror/lang-python")).python(),
  },
  {
    id: "kotlin",
    label: "Kotlin",
    extensions: ["kt"],
    load: async () => {
      const [{ StreamLanguage }, { kotlin }] = await Promise.all([
        import("@codemirror/language"),
        import("@codemirror/legacy-modes/mode/clike"),
      ]);
      return StreamLanguage.define(kotlin);
    },
  },
];

export function languageForFile(name: string): LanguageRegistration | undefined {
  const extension = name.split(".").pop()?.toLowerCase() ?? "";
  return languages.find((language) => language.extensions.includes(extension));
}
