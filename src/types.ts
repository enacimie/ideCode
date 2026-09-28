export type Severity = "error" | "warning" | "note";

export type Diagnostic = {
  severity: Severity;
  message: string;
  file: string | null;
  line: number | null;
  column: number | null;
};

export type BuildResult = {
  success: boolean;
  diagnostics: Diagnostic[];
};

export type RunOutcome = {
  exit_code: number | null;
  timed_out: boolean;
};

export type RunChunk = {
  stream: "stdout" | "stderr";
  line: string;
};

export type OutputLine = RunChunk;

export type SourceEntry = {
  path: string;
  relative: string;
  name: string;
  content: string;
};

export type ProjectSnapshot = {
  root: string;
  language: string;
  files: SourceEntry[];
};

export type LanguageInfo = {
  id: string;
  name: string;
  extensions: string[];
  compileLabel: string;
  entryLabel: string;
};

export type ExampleInfo = {
  id: string;
  name: string;
  files: number;
};

export type ClassKind = "class" | "interface" | "enum" | "record" | "annotation" | "module";

export type Visibility = "public" | "protected" | "package" | "private";

export type Multiplicity = "one" | "zero_or_one" | "zero_or_many";

export type ParameterModel = {
  name: string;
  ty: string;
};

export type FieldModel = {
  name: string;
  ty: string;
  targets: string[];
  multiplicity: Multiplicity;
  visibility: Visibility;
  is_static: boolean;
  is_final: boolean;
  line: number;
};

export type EnumConstantModel = {
  name: string;
  line: number;
};

export type MethodModel = {
  name: string;
  return_ty: string;
  visibility: Visibility;
  is_static: boolean;
  is_abstract: boolean;
  is_constructor: boolean;
  is_async: boolean;
  type_parameters: string[];
  throws: string[];
  parameters: ParameterModel[];
  line: number;
};

export type FunctionModel = {
  name: string;
  return_ty: string;
  is_async: boolean;
  parameters: ParameterModel[];
  line: number;
};

export type ClassModel = {
  name: string;
  simple_name: string;
  outer: string | null;
  kind: ClassKind;
  package: string | null;
  is_abstract: boolean;
  type_parameters: string[];
  extends: string[];
  implements: string[];
  enum_constants: EnumConstantModel[];
  fields: FieldModel[];
  methods: MethodModel[];
  functions: FunctionModel[];
  uses: string[];
  line: number;
  file: string;
};

export type DiagramResult = {
  language: string;
  mermaid: string;
  classes: ClassModel[];
};

export type GeneratedFile = {
  relative: string;
  content: string;
};

export type WriteOutcome = {
  written: SourceEntry[];
  conflicts: string[];
};

export type RevealTarget = {
  path: string;
  line: number;
  nonce: number;
};

export type Notice = {
  kind: "info" | "success" | "error";
  message: string;
};
