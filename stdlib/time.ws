// Whisper Time Standard Library
// Requires: --allow-env (uses now opcode, no capability needed)

: now        { now } ;
: sleep      { sleep } ;
: sleep-secs { 1000 * sleep } ;

export now
export sleep
export sleep-secs
