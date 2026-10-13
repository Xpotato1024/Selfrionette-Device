param([Parameter(Mandatory=$true)][int]$AppProcess,[string]$ItemText='',[string]$CapturePath='')
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class NativeMenuEntry { public uint Id; public string Text; public bool Submenu; }
public static class NativeMenuProbe {
 [DllImport("user32.dll")] public static extern IntPtr GetMenu(IntPtr h);
 [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr h);
 [DllImport("user32.dll")] static extern IntPtr GetSubMenu(IntPtr h,int i);
 [DllImport("user32.dll")] static extern uint GetMenuItemID(IntPtr h,int i);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetMenuString(IntPtr h,uint i,StringBuilder text,int length,uint flags);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint message,IntPtr w,IntPtr l);
 public static NativeMenuEntry[] Entries(IntPtr menu) {
  var list=new List<NativeMenuEntry>();
  for(int i=0;i<GetMenuItemCount(menu);i++) {
   var text=new StringBuilder(512);GetMenuString(menu,(uint)i,text,512,0x400);
   IntPtr sub=GetSubMenu(menu,i);list.Add(new NativeMenuEntry{Id=GetMenuItemID(menu,i),Text=text.ToString(),Submenu=sub!=IntPtr.Zero});
   if(sub!=IntPtr.Zero)list.AddRange(Entries(sub));
  }return list.ToArray();
 }
}
"@
$process=Get-Process -Id $AppProcess
$window=$process.MainWindowHandle
if($window -eq 0){throw 'Owned application window is not available'}
$menu=[NativeMenuProbe]::GetMenu($window)
if($menu -eq 0){throw 'No real Win32 menu attached to the application'}
$entries=[NativeMenuProbe]::Entries($menu)
if($ItemText){
 $matches=@($entries | Where-Object {-not $_.Submenu -and ($_.Text -split "	")[0] -eq $ItemText})
 if($matches.Count -ne 1){throw ('Expected one menu item: '+$ItemText)}
 if(-not [NativeMenuProbe]::PostMessage($window,0x111,[IntPtr]::new($matches[0].Id),[IntPtr]::Zero)){throw 'Menu activation failed'}
}
if($CapturePath){
 Add-Type -AssemblyName System.Drawing
 Add-Type @"
using System;
using System.Runtime.InteropServices;
public struct CaptureRect {public int Left,Top,Right,Bottom;}
public static class WindowCapture {
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out CaptureRect rect);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
}
"@
 $rect=New-Object CaptureRect
 if(-not [WindowCapture]::GetWindowRect($window,[ref]$rect)){throw 'Cannot read owned window bounds'}
 $bitmap=New-Object Drawing.Bitmap(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top))
 $graphics=[Drawing.Graphics]::FromImage($bitmap);$dc=$graphics.GetHdc()
 try {if(-not [WindowCapture]::PrintWindow($window,$dc,2)){throw 'Owned window capture failed'}}
 finally {$graphics.ReleaseHdc($dc)}
 try {$bitmap.Save($CapturePath,[Drawing.Imaging.ImageFormat]::Png)}finally{$graphics.Dispose();$bitmap.Dispose()}
}
$entries | ConvertTo-Json -Depth 3 -Compress
