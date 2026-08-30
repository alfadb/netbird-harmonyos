export interface GoProbeResult {
  ok: boolean;
  stage: string;
  loaderErrno: number;
  loaderError: string;
  hello: number;
  runtimeBytes: number;
  pid: number;
}

export const runGoProbe: () => GoProbeResult;
