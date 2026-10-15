/* Standalone POSIX DNS probe. */
#define _POSIX_C_SOURCE 200809L
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netdb.h>
#include <poll.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>
static int64_t deadline;
static void expired(int s) {
    static const char m[]="timeout: overall deadline exceeded\n";
    (void)s; (void)write(2,m,sizeof(m)-1); _exit(3);
}
static int64_t now_ms(void) {
    struct timespec t;
    if (clock_gettime(CLOCK_MONOTONIC,&t)) { perror("clock_gettime"); exit(3); }
    return (int64_t)t.tv_sec*1000+t.tv_nsec/1000000;
}
static unsigned u16(const unsigned char *p) { return (unsigned)p[0]*256+p[1]; }
static void put16(unsigned char *p,unsigned v) { p[0]=(unsigned char)(v>>8); p[1]=(unsigned char)v; }
static int ready(int fd,short events) {
    for (;;) {
        int64_t left=deadline-now_ms(); struct pollfd p={fd,events,0};
        if (left<=0) { errno=ETIMEDOUT; return -1; }
        int r=poll(&p,1,(int)left);
        if (r>0) return 0;
        if (!r) { errno=ETIMEDOUT; return -1; }
        if (errno!=EINTR) return -1;
    }
}
static int transfer(int fd,unsigned char *p,size_t n,int sending) {
    while (n) {
        if (ready(fd,sending?POLLOUT:POLLIN)) return -1;
        ssize_t r=sending?send(fd,p,n,0):recv(fd,p,n,0);
        if (r<0) { if (errno==EINTR||errno==EAGAIN||errno==EWOULDBLOCK) continue; return -1; }
        if (!r) { errno=ECONNRESET; return -1; }
        p+=r; n-=(size_t)r;
    }
    return 0;
}
static int hostname(const char *s,unsigned char *wire,size_t *len,char *canon) {
    size_t n=strnlen(s,255),pos=0,out=0,c=0;
    if (!n||n>=255) return -1;
    if (s[n-1]=='.') n--;
    if (!n||n>253) return -1;
    while (pos<n) {
        size_t start=pos;
        while (pos<n&&s[pos]!='.') pos++;
        size_t l=pos-start;
        if (!l||l>63||s[start]=='-'||s[pos-1]=='-') return -1;
        wire[out++]=(unsigned char)l;
        if (c) canon[c++]='.';
        for (size_t j=start;j<pos;j++) {
            unsigned char ch=(unsigned char)s[j];
            if (!((ch>='a'&&ch<='z')||(ch>='A'&&ch<='Z')||(ch>='0'&&ch<='9')||ch=='-')) return -1;
            if (ch>='A'&&ch<='Z') ch=(unsigned char)(ch+32);
            wire[out++]=ch; canon[c++]=(char)ch;
        }
        if (pos<n) { pos++; if (pos==n) return -1; }
    }
    wire[out++]=0; canon[c]=0; *len=out; return out<=255?0:-1;
}
/* Backward compression pointers, bounded hops/expanded size, escaped output. */
static int name(const unsigned char *b,size_t n,size_t *cursor,char *out,size_t cap) {
    size_t p=*cursor,end=0,used=0,expanded=1,hops=0; int jumped=0;
    for (;;) {
        if (p>=n||++hops>n) return -1;
        unsigned l=b[p++];
        if ((l&0xc0)==0xc0) {
            if (p>=n) return -1;
            size_t target=((l&63)<<8)|b[p++];
            if (target>=n||target>=p-2) return -1;
            if (!jumped) end=p;
            jumped=1; p=target; continue;
        }
        if (l&0xc0) return -1;
        if (!l) { if (!jumped) end=p; if (used>=cap) return -1; out[used]=0; *cursor=end; return 0; }
        if (l>n-p||expanded+l+1>255) return -1;
        expanded+=l+1;
        if (used) { if (used+1>=cap) return -1; out[used++]='.'; }
        for (unsigned j=0;j<l;j++) {
            unsigned ch=b[p++];
            if ((ch>='a'&&ch<='z')||(ch>='A'&&ch<='Z')||(ch>='0'&&ch<='9')||ch=='-'||ch=='_') {
                if (used+1>=cap) return -1;
                out[used++]=(char)ch;
            } else {
                if (used+4>=cap) return -1;
                out[used++]='\\'; out[used++]=(char)('0'+ch/100);
                out[used++]=(char)('0'+ch/10%10); out[used++]=(char)('0'+ch%10);
            }
        }
    }
}
static const char *rcode_name(unsigned r) {
    static const char *const names[]={"NOERROR","FORMERR","SERVFAIL","NXDOMAIN","NOTIMP","REFUSED"};
    return r<6?names[r]:"OTHER";
}
static int response(const unsigned char *b,size_t n,unsigned id,const char *host,unsigned type,int print) {
    char owner[1024],value[1024]; size_t p=12;
    if (n<12||u16(b)!=id||!(b[2]&0x80)||(b[2]&0x78)||u16(b+4)!=1) return -1;
    if (name(b,n,&p,owner,sizeof(owner))||strcasecmp(owner,host)||n-p<4||u16(b+p)!=type||u16(b+p+2)!=1) return -1;
    p+=4;
    unsigned an=u16(b+6),ns=u16(b+8),ar=u16(b+10);
    if (print) printf("DNS id=%u rcode=%u(%s) TC=%u answers=%u authority=%u additional=%u\n",id,b[3]&15,rcode_name(b[3]&15),(b[2]>>1)&1,an,ns,ar);
    for (unsigned i=0;i<an+ns+ar;i++) {
        if (name(b,n,&p,owner,sizeof(owner))||n-p<10) return -1;
        unsigned t=u16(b+p),cls=u16(b+p+2),rdlen=u16(b+p+8);
        uint32_t ttl=(uint32_t)b[p+4]<<24|(uint32_t)b[p+5]<<16|(uint32_t)b[p+6]<<8|b[p+7]; p+=10;
        if (rdlen>n-p) return -1;
        value[0]=0;
        if (cls==1&&(t==1||t==28)) {
            if (rdlen!=(t==1?4:16)||!inet_ntop(t==1?AF_INET:AF_INET6,b+p,value,sizeof(value))) return -1;
        } else if (t==5||t==2||t==12) {
            size_t q=p;
            if (name(b,n,&q,value,sizeof(value))||q!=p+rdlen) return -1;
        }
        if (print&&i<an) printf("answer name=%s type=%u class=%u ttl=%u rdlength=%u%s%s\n",owner,t,cls,(unsigned)ttl,rdlen,value[0]?" value=":"",value);
        p+=rdlen;
    }
    return p==n?0:-1;
}
static int system_lookup(const char *host) {
    struct addrinfo hints,*list=NULL;
    memset(&hints,0,sizeof(hints)); hints.ai_family=AF_UNSPEC; hints.ai_socktype=SOCK_STREAM; errno=0;
    int rc=getaddrinfo(host,NULL,&hints,&list),saved=errno;
    printf("getaddrinfo code=%d message=%s errno=%d\n",rc,rc?gai_strerror(rc):"OK",rc==EAI_SYSTEM?saved:0);
    if (rc) return 5;
    for (struct addrinfo *p=list;p;p=p->ai_next) {
        char text[INET6_ADDRSTRLEN]; const void *addr=NULL;
        if (p->ai_family==AF_INET&&p->ai_addrlen>=sizeof(struct sockaddr_in)) addr=&((struct sockaddr_in *)p->ai_addr)->sin_addr;
        if (p->ai_family==AF_INET6&&p->ai_addrlen>=sizeof(struct sockaddr_in6)) addr=&((struct sockaddr_in6 *)p->ai_addr)->sin6_addr;
        if (addr&&inet_ntop(p->ai_family,addr,text,sizeof(text))) printf("%s %s\n",p->ai_family==AF_INET?"A":"AAAA",text);
    }
    freeaddrinfo(list); return 0;
}
static int number(const char *s,int max) {
    char *end; errno=0; long v=strtol(s,&end,10);
    return errno||!*s||*end||v<1||v>max?-1:(int)v;
}
int main(int argc,char **argv) {
    const char *server=NULL,*host=NULL; int system=0,tcp=-1,type=0,port=53,timeout=3000;
    for (int i=1;i<argc;i++) {
        if (!strcmp(argv[i],"--system")) { if (system) goto usage; system=1; }
        else if (!strcmp(argv[i],"--server")&&i+1<argc) { if (server) goto usage; server=argv[++i]; }
        else if (!strcmp(argv[i],"--udp")) { if (tcp!=-1) goto usage; tcp=0; }
        else if (!strcmp(argv[i],"--tcp")) { if (tcp!=-1) goto usage; tcp=1; }
        else if (!strcmp(argv[i],"--type")&&i+1<argc) { if (type) goto usage; i++; type=!strcmp(argv[i],"A")?1:!strcmp(argv[i],"AAAA")?28:0; if (!type) goto usage; }
        else if (!strcmp(argv[i],"--port")&&i+1<argc) { port=number(argv[++i],65535); if (port<0) goto usage; }
        else if (!strcmp(argv[i],"--timeout-ms")&&i+1<argc) { timeout=number(argv[++i],60000); if (timeout<0) goto usage; }
        else { if (host||argv[i][0]=='-') goto usage; host=argv[i]; }
    }
    unsigned char query[512]={0},answer[65535],wire[255]; char canon[254]; size_t wn;
    if (!host||hostname(host,wire,&wn,canon)) goto usage;
    if (system?(server||tcp!=-1||type||port!=53):(!server||tcp==-1||!type)) goto usage;
    struct sigaction sa; memset(&sa,0,sizeof(sa)); sa.sa_handler=expired; sigemptyset(&sa.sa_mask);
    if (sigaction(SIGALRM,&sa,NULL)) { perror("sigaction"); return 3; }
    signal(SIGPIPE,SIG_IGN);
    struct itimerval timer; memset(&timer,0,sizeof(timer));
    timer.it_value.tv_sec=timeout/1000; timer.it_value.tv_usec=(timeout%1000)*1000; deadline=now_ms()+timeout;
    if (setitimer(ITIMER_REAL,&timer,NULL)) { perror("setitimer"); return 3; }
    if (system) return system_lookup(host);
    struct sockaddr_in dest; memset(&dest,0,sizeof(dest)); dest.sin_family=AF_INET; dest.sin_port=htons((uint16_t)port);
    if (inet_pton(AF_INET,server,&dest.sin_addr)!=1) goto usage;
    unsigned id=(unsigned)((getpid()^(unsigned)now_ms())&65535);
    put16(query,id); query[2]=1; put16(query+4,1); memcpy(query+12,wire,wn);
    put16(query+12+wn,(unsigned)type); put16(query+14+wn,1); size_t qlen=16+wn;
    int fd=socket(AF_INET,tcp?SOCK_STREAM:SOCK_DGRAM,0);
    if (fd<0) { perror("socket"); return 3; }
    if (fcntl(fd,F_SETFL,O_NONBLOCK)<0) goto transport;
    int cr=connect(fd,(struct sockaddr *)&dest,sizeof(dest));
    if (cr<0) {
        if (errno!=EINPROGRESS||ready(fd,POLLOUT)) goto transport;
        int error=0; socklen_t sz=sizeof(error);
        if (getsockopt(fd,SOL_SOCKET,SO_ERROR,&error,&sz)) goto transport;
        if (error) { errno=error; goto transport; }
    }
    printf("query server=%s port=%d transport=%s type=%s host=%s timeout_ms=%d\n",server,port,tcp?"TCP":"UDP",type==1?"A":"AAAA",canon,timeout);
    size_t n;
    if (tcp) {
        unsigned char prefix[2]; put16(prefix,(unsigned)qlen);
        if (transfer(fd,prefix,2,1)||transfer(fd,query,qlen,1)||transfer(fd,prefix,2,0)) goto transport;
        n=u16(prefix);
        if (!n) { close(fd); fprintf(stderr,"invalid DNS: empty TCP frame\n"); return 4; }
        if (transfer(fd,answer,n,0)) goto transport;
    } else {
        for (;;) {
            if (ready(fd,POLLOUT)) goto transport;
            ssize_t r=send(fd,query,qlen,0);
            if (r<0&&(errno==EINTR||errno==EAGAIN||errno==EWOULDBLOCK)) continue;
            if (r!=(ssize_t)qlen) { if (r>=0) errno=EIO; goto transport; }
            break;
        }
        for (;;) {
            if (ready(fd,POLLIN)) goto transport;
            ssize_t r=recv(fd,answer,sizeof(answer),0);
            if (r<0) { if (errno==EINTR||errno==EAGAIN||errno==EWOULDBLOCK) continue; goto transport; }
            n=(size_t)r;
            if (n>=2&&u16(answer)!=id) continue;
            break;
        }
    }
    close(fd);
    if (response(answer,n,id,canon,(unsigned)type,0)) { fprintf(stderr,"invalid DNS response (header/question/records)\n"); return 4; }
    (void)response(answer,n,id,canon,(unsigned)type,1); return 0;
transport:
    fprintf(stderr,"transport failure errno=%d message=%s\n",errno,strerror(errno)); close(fd); return 3;
usage:
    fprintf(stderr,"Usage: dns-probe --system hostname [--timeout-ms 1..60000]\n       dns-probe --server IPv4 --udp|--tcp --type A|AAAA hostname [--port 1..65535] [--timeout-ms 1..60000]\n"); return 2;
}
