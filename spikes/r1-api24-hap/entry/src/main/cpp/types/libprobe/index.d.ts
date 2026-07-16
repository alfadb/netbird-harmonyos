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

export const ping: () => string;
export const version: () => string;
export const runDynamicTlsProbe: () => DynamicTlsProbeResult;
