export interface E2NetworkRoundResult {
  pid: number;
  tid: number;
  round: number;
  tcpLoopBytes: number;
  tcpLoopHash: number;
  tcpLoopEpollEvents: number;
  tcpLoopTimeoutMs: number;
  tcpLoopPeerClose: number;
  tcpLoopRefusedErrno: number;
  tcpLoopListenerCloseErrno: number;
  tcpLoopServerReads: number;
  tcpLoopClientReads: number;
  udpLoopClientToServer: number;
  udpLoopServerToClient: number;
  udpLoopClientAggregate: number;
  udpLoopServerAggregate: number;
  hostRoute: string;
  hostAddress: string;
  hostTcpBytes: number;
  hostTcpHash: number;
  hostUdpDatagrams: number;
  hostUdpAggregate: number;
  localhostCount: number;
  invalidDnsCode: number;
}

export const version: () => string;
export const runNetworkRound: (round: number) => E2NetworkRoundResult;
