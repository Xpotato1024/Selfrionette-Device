# Test harness only. Interact exclusively with the process started here and its children.
param([Parameter(Mandatory=$true)][string]$Installer,[Parameter(Mandatory=$true)][string]$Report,[switch]$ExpectLaunchError,[switch]$CancelBeforeUninstall)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
Add-Type -TypeDefinition @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class InstallerWindow {
  public IntPtr Handle; public uint Pid; public string Text; public string Class; public int Id; public long Style; public bool Enabled;
}
public static class InstallerUi {
  public delegate bool EnumProc(IntPtr h,IntPtr data);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f,IntPtr p);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h,EnumProc f,IntPtr p);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool IsWindowEnabled(IntPtr h);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h,StringBuilder s,int n);
  [DllImport("user32.dll")] static extern int GetDlgCtrlID(IntPtr h);
  [DllImport("user32.dll")] static extern int GetWindowLong(IntPtr h,int n);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint msg,IntPtr w,IntPtr l);
  [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr h,uint msg,IntPtr w,IntPtr l,uint flags,uint timeout,out IntPtr result);
  public static IntPtr SendMessage(IntPtr h,uint msg,IntPtr w,IntPtr l) {IntPtr result;SendMessageTimeout(h,msg,w,l,2,200,out result);return result;}
  static InstallerWindow Read(IntPtr h) { uint p;GetWindowThreadProcessId(h,out p);var s=new StringBuilder(2048);GetWindowText(h,s,s.Capacity);var c=new StringBuilder(128);GetClassName(h,c,c.Capacity);return new InstallerWindow{Handle=h,Pid=p,Text=s.ToString(),Class=c.ToString(),Id=GetDlgCtrlID(h),Style=GetWindowLong(h,-16),Enabled=IsWindowEnabled(h)}; }
  public static InstallerWindow[] Windows(uint[] pids) { var a=new List<InstallerWindow>();EnumWindows((h,p)=>{if(IsWindowVisible(h)){var v=Read(h);if(Array.IndexOf(pids,v.Pid)>=0)a.Add(v);}return true;},IntPtr.Zero);return a.ToArray(); }
  public static InstallerWindow[] Children(IntPtr h) { var a=new List<InstallerWindow>();EnumChildWindows(h,(x,p)=>{if(IsWindowVisible(x))a.Add(Read(x));return true;},IntPtr.Zero);return a.ToArray(); }
}
"@
$root=Start-Process -FilePath $Installer -PassThru
$owned=[Collections.Generic.HashSet[uint32]]::new();[void]$owned.Add($root.Id)
$events=[Collections.Generic.List[object]]::new();$sawError=$false;$sawUninstaller=$false;$cancelled=$false
$clock=[Diagnostics.Stopwatch]::StartNew();$lastAction=[DateTime]::MinValue
try {
  while ($clock.Elapsed.TotalSeconds -lt 150) {
    foreach($parent in @($owned)){Get-CimInstance Win32_Process -Filter "ParentProcessId=$parent" | ForEach-Object {[void]$owned.Add([uint32]$_.ProcessId)}}
    $windows=[InstallerUi]::Windows([uint32[]]@($owned))
    foreach($window in $windows){
      $children=[InstallerUi]::Children($window.Handle)
      $content=($children.Text -join ' | ')
      if($window.Text -match 'NSIS Error' -or $content -match 'Error launching installer'){
        $sawError=$true;$events.Add(@{kind='launch-error';pid=$window.Pid;title=$window.Text;content=$content})
        $ok=$children|Where-Object {$_.Class -eq 'Button' -and $_.Id -eq 1}|Select-Object -First 1
        if($ok){[void][InstallerUi]::PostMessage($ok.Handle,0xF5,[IntPtr]::Zero,[IntPtr]::Zero)}
        continue
      }
      if($window.Pid -ne $root.Id){$sawUninstaller=$true}
      if(([DateTime]::Now-$lastAction).TotalMilliseconds -lt 650){continue}
      if($sawError -or $CancelBeforeUninstall){
        $cancel=$children|Where-Object {$_.Class -eq 'Button' -and $_.Enabled -and $_.Id -eq 2}|Select-Object -First 1
        $yes=$children|Where-Object {$_.Class -eq 'Button' -and $_.Enabled -and $_.Id -eq 6}|Select-Object -First 1
        $button=if($yes){$yes}else{$cancel}
        if($button){[void][InstallerUi]::PostMessage($button.Handle,0xF5,[IntPtr]::Zero,[IntPtr]::Zero);$lastAction=[DateTime]::Now;$cancelled=$true}
        continue
      }
      # Do not start the app, create shortcuts, or request deleting app data.
      foreach($box in $children|Where-Object {$_.Class -eq 'Button' -and (($_.Style -band 15) -in @(2,3))}){
        [void][InstallerUi]::SendMessage($box.Handle,0xF1,[IntPtr]::Zero,[IntPtr]::Zero)
      }
      $next=$children|Where-Object {$_.Class -eq 'Button' -and $_.Enabled -and $_.Id -eq 1 -and $_.Text -match '(?i)next|install|finish|close|\u6b21|\u5b8c\u4e86|\u9589\u3058|\u30a4\u30f3\u30b9\u30c8\u30fc\u30eb'}|Select-Object -First 1
      if($next){
        $events.Add(@{kind='advance';pid=$window.Pid;title=$window.Text;button=$next.Text;content=$content})
        [void][InstallerUi]::PostMessage($next.Handle,0xF5,[IntPtr]::Zero,[IntPtr]::Zero);$lastAction=[DateTime]::Now
      }
    }
    $root.Refresh();if($root.HasExited){break};Start-Sleep -Milliseconds 150
  }
  if(-not $root.HasExited){throw 'Owned installer did not finish within 150 seconds'}
  $result=@{exit_code=$root.ExitCode;launch_error=$sawError;child_uninstaller_observed=$sawUninstaller;cancelled=$cancelled;events=$events}
  [IO.File]::WriteAllText($Report,($result|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
  if($ExpectLaunchError){if(-not $sawError){throw 'Expected old cross-publisher launch error was not reproduced'}}
  elseif($sawError){throw 'Unexpected NSIS launch error'}
  elseif(-not $CancelBeforeUninstall -and $root.ExitCode -ne 0){throw "Installer failed: $($root.ExitCode)"}
  Write-Output "Installer UI test finished: $Report"
} finally {
  if(-not (Test-Path $Report)){[IO.File]::WriteAllText($Report,(@{events=$events;launch_error=$sawError}|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))}
}
