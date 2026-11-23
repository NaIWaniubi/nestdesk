Add-Type @"
using System;using System.Text;using System.Runtime.InteropServices;
public class WinProbe3 {
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
 public delegate bool EnumProc(IntPtr h, IntPtr l);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
 [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out int v, int s);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
 public static void Run() {
  uint target = 0;
  foreach (var p in System.Diagnostics.Process.GetProcessesByName("nestdesk-app")) { target = (uint)p.Id; break; }
  if (target == 0) { Console.WriteLine("no nestdesk-app process"); return; }
  Console.WriteLine("pid=" + target);
  EnumWindows((h, l) => {
   uint pid; GetWindowThreadProcessId(h, out pid);
   if (pid != target) return true;
   RECT r; GetWindowRect(h, out r);
   var t = new StringBuilder(128); GetWindowTextW(h, t, 128);
   int cloak=-1, live=1; DwmGetWindowAttribute(h, 14, out cloak, 4); DwmGetWindowAttribute(h, 4, out live, 4);
   Console.WriteLine("hwnd=" + (long)h + " vis=" + IsWindowVisible(h) + " iconic=" + IsIconic(h) + " rect=(" + r.L + "," + r.T + "," + r.R + "," + r.B + ") cloaked=" + cloak + " fcVis=" + (live!=0) + " title=" + t);
   return true;
  }, IntPtr.Zero);
 }
}
"@
[WinProbe3]::Run()
