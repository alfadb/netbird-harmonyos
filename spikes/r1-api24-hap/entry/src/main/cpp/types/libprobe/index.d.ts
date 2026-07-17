export interface TlsModelResult {
  attempted: boolean;
  required: boolean;
  ok: boolean;
  expectedBlock: boolean;
  environmentDrift: boolean;
  preThreadCreatedBeforeDlopen: boolean;
  postThreadCreatedAfterDlopen: boolean;
  model: string;
  library: string;
  functionalStatus: string;
  stage: string;
  detail: string;
  loaderError: string;
  loaderErrno: number;
  mainValue: number;
  preThreadValue: number;
  postThreadValue: number;
  mainInitial: number;
  preThreadInitial: number;
  postThreadInitial: number;
  mainReset: number;
  preThreadReset: number;
  postThreadReset: number;
  mainIterations: number;
  preThreadIterations: number;
  postThreadIterations: number;
}

export interface DynamicTlsProbeResult {
  ok: boolean;
  ieBlocked: boolean;
  environmentDrift: boolean;
  tier1Pass: boolean;
  verdict: string;
  stopReason: string;
  initialExec: TlsModelResult;
  globalDynamic: TlsModelResult;
  tlsDesc: TlsModelResult;
  localDynamic: TlsModelResult;
}

export interface GoThreadResult {
  started: boolean;
  resolved: boolean;
  helloOk: boolean;
  runtimeOk: boolean;
  netDialOk: boolean;
  ok: boolean;
  role: string;
  stage: string;
  detail: string;
  hello: number;
  runtimeBytes: number;
  netDialCode: number;
}

export interface GoProbeResult {
  ok: boolean;
  preThreadCreatedBeforeDlopen: boolean;
  dlopenLoaded: boolean;
  postThreadCreatedAfterDlopen: boolean;
  verdict: string;
  stage: string;
  detail: string;
  loaderError: string;
  loaderErrno: number;
  processId: number;
  preThread: GoThreadResult;
  postThread: GoThreadResult;
}

export const ping: () => string;
export const version: () => string;
export const runDynamicTlsProbe: () => DynamicTlsProbeResult;
export const runGoProbe: () => GoProbeResult;
