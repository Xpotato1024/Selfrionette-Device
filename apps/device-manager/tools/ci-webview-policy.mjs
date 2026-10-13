// Test-only override for elevated hosted runners; not bundled in the application.
// WebView2 >=150 ignores environment overrides for elevated hosts.
// https://learn.microsoft.com/microsoft-edge/webview2/concepts/security
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
export function ciPolicyEnabled(env) {
  const requested=env.SELFRIONETTE_CI_WEBVIEW_POLICY==='1';
  if(requested&&(env.GITHUB_ACTIONS!=='true'||env.RUNNER_ENVIRONMENT!=='github-hosted'))throw Error('WebView2 test policy is restricted to the ephemeral GitHub-hosted runner');
  return requested;
}
function policy(mode,extra={}) {
  const script=fileURLToPath(new URL('./ci-webview-policy.ps1',import.meta.url));
  const result=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-File',script,'-Mode',mode],{
    env:{...process.env,...extra},encoding:'utf8',timeout:10000,maxBuffer:1024*1024});
  if(result.error)throw result.error;
  if(result.status!==0)throw Error('CI WebView2 policy: '+result.stderr);
}
export function installCIPolicy(port,profile) {
  if(!ciPolicyEnabled(process.env))return false;
  if(!Number.isInteger(port)||port<1024||port>65535)throw Error('Invalid CDP port');
  policy('install',{SRN_POLICY_ARGUMENTS:'--remote-debugging-port='+port+' --remote-debugging-address=127.0.0.1',SRN_POLICY_PROFILE:profile});
  return true;
}
export function removeCIPolicy() {
  if(ciPolicyEnabled(process.env))policy('remove');
}
