export interface FdStatus {
  open: boolean;
  errno: number;
}

export const status: (fd: number) => FdStatus;
