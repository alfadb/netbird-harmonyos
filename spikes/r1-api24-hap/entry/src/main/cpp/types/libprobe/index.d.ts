export interface ResourceSnapshot {
  pid: number;
  tid: number;
  fdCount: number;
  threadCount: number;
}

export interface BufferHashResult {
  hash: number;
  length: number;
  first: number;
  last: number;
  nativeTid: number;
}

export interface FdPair {
  fdA: number;
  fdB: number;
  creatorTid: number;
}

export interface FdTransferResult {
  fdA: number;
  fdB: number;
  duplicateFd: number;
  written: number;
  read: number;
  closeOriginalA: number;
  closeDuplicate: number;
  closeOriginalB: number;
  duplicateCloseA: number;
  duplicateCloseErrnoA: number;
  duplicateCloseB: number;
  duplicateCloseErrnoB: number;
  payloadHash: number;
  payload: string;
  nativeTid: number;
}

export interface AsyncRoundResult {
  round: number;
  producerTid: number;
  queued: number;
  callbackStatus: number;
  joinStatus: number;
}

export type AsyncCallback = (round: number, sequence: number, payload: string, payloadHash: number,
  producerTid: number, consumerTid: number) => void;

export const ping: () => string;
export const version: () => string;
export const resourceSnapshot: () => ResourceSnapshot;
export const hashBuffer: (buffer: Uint8Array) => BufferHashResult;
export const createFdPair: () => FdPair;
export const transferFdOwnership: (fdA: number, fdB: number, round: number) => FdTransferResult;
export const initializeAsync: (callback: AsyncCallback) => ResourceSnapshot;
export const startAsyncRound: (round: number) => void;
export const completeAsyncRound: (round: number) => AsyncRoundResult;
export const shutdownAsync: () => ResourceSnapshot;
