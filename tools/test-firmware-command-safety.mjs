// Emit a native test of the actual archived command blocks and modern dispatcher.
// No archive writes, device I/O, compiler subprocess, or browser launch.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const read=p=>readFileSync(resolve(repo,p),'utf8');
function block(source,marker){
 const start=source.indexOf(marker);if(start<0)throw Error('Missing dispatcher '+marker);
 const open=source.indexOf('{',start);let depth=1,end=open+1;
 for(;depth&&end<source.length;end++){if(source[end]==='{')depth++;if(source[end]==='}')depth--;}
 if(depth)throw Error('Unbalanced source block');return source.slice(open,end);
}
const root='firmware/legacy/xpotato-sim/arduino/legacy_selfrionette/';
const original=read(root+'loadcell_7ch_legacy/src/main.cpp');
if(!/bool\s+isLeader\s*=\s*true/.test(original))throw Error('Archive leader default changed');
const cleaned=read(root+'loadcell_7ch_pro_micro/src/main.cpp');
const modern=read('firmware/loadcell_7ch/src/main.cpp');
const cpp=`#include <cmath>
#include <cstring>
#include <string>
#include <cstdio>
#include <cstdlib>
#include "${resolve(repo,'firmware/shared/firmware_protocol_v3.h').replaceAll('\\','/')}"
#define CHECK(x) do { if (!(x)) { fprintf(stderr,"FAIL line %d: %s\\n",__LINE__,#x); exit(1); } } while(0)
#define F(x) x
using namespace selfrionette::protocol3;
static int mutations=0, info2=0, info3=0;
struct SerialFake { std::string bytes; size_t at=0;
 int available(){return int(bytes.size()-at);} char read(){return bytes[at++];}
 void feed(const char* s){bytes=s;at=0;}
 template<class T> void print(T){} template<class T> void println(T){} void println(){}
 void write(const char*){mutations++;}
} Serial,Serial1;
void delay(int){} void delayMicroseconds(int){}
struct Scale { bool is_ready(){return true;} long read(){mutations++;return 1;} } scales[7];
int LOADCELL_DOUT_PINS[7]={}; double offset[7]={},prev_values[7]={}; bool isLeader=true;
void originalCommand() ${block(original,'if (Serial.available())')}
void emitStatus(const char*){} void emitWarn(const char*){}
void calibrateAllChannels(){mutations++;}
void cleanedCommand() ${block(cleaned,'void handleSerialCommands()')}
void emitDeviceInfo(){info2++;} void emitDeviceInfo3(uint32_t){info3++;}
void cancelTare(const char*){mutations++;} void invalidateTare(){mutations++;}
void handleProvisionCommand(const char*){mutations++;}
void handleStream3(const Command&){mutations++;} void handleSettings3(const Command&){mutations++;}
void beginTare(const Command&){mutations++;} void emitError3(uint32_t,const char*){}
void executeLegacyCommand(char* command) ${block(modern,'void executeLegacyCommand(')}
void executeV3Command(const Command& command) ${block(modern,'void executeV3Command(')}
char g_command_buffer[88]={}; uint8_t g_command_length=0; bool g_command_overflow=false;
void finishCommand() ${block(modern,'void finishCommand()')}
void modernCommand(const char* s){strcpy(g_command_buffer,s);g_command_length=uint8_t(strlen(s));finishCommand();}
int main(){
 const char* probes[]={"?2\\n","?3,123\\n","?3,4294967295\\n"};
 for(const char* probe:probes){
   mutations=0;Serial.feed(probe);while(Serial.available())originalCommand();CHECK(mutations==0);
   Serial.feed(probe);cleanedCommand();CHECK(mutations==0);
 }
 Serial.feed("info3,123\\n");while(Serial.available())originalCommand();CHECK(mutations>0);
 mutations=0;Serial.feed("c\\n");cleanedCommand();CHECK(mutations>0);
 mutations=0;modernCommand("?2");CHECK(info2==1&&info3==0&&mutations==0);
 modernCommand("?3,123");CHECK(info3==1&&mutations==0);
 modernCommand("info");modernCommand("info3,124");CHECK(info2==2&&info3==2&&mutations==0);
 modernCommand("?3,0");modernCommand("?3,1,extra");CHECK(info3==2&&mutations==0);
 puts("PASS: both frozen byte dispatchers, legacy i positive control, modern aliases and unchanged info dispatch");
}
`;
if(process.argv[2]!=='--emit-cpp'||!process.argv[3])throw Error('Usage: node tools/test-firmware-command-safety.mjs --emit-cpp ABSOLUTE_OUTPUT.cpp');
writeFileSync(resolve(process.argv[3]),cpp);
