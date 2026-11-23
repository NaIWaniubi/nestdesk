Add-Type @"
using System;using System.Text;using System.Runtime.InteropServices;
public class LayerProbe2 {
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
 public delegate bool EnumProc(IntPtr h, IntPtr l);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll", SetLastError=true)] public static extern int GetWindowLong(IntPtr h, int i);
 [DllImport("user32.dll")] public static extern bool GetLayeredWindowAttributes(IntPtr h, out uint key, out byte alpha, out uint flags);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
 public static void Run() {
  uint target = 0;
  foreach (var p in System.Diagnostics.Process.GetProcessesByName("nestdesk-app")) { target = (uint)p.Id; break; }
  if (target == 0) { Console.WriteLine("no process"); return; }
  EnumWindows((h, l) => {
   uint pid; GetWindowThreadProcessId(h, out pid);
   if (pid != target) return true;
   RECT r; GetWindowRect(h, out r);
   if (r.R - r.L < 60) return true;
   int ex = GetWindowLong(h, -20);
   bool layered = (ex & 0x80000) != 0, noredir = (ex & 0x40000) != 0;
   string extra = "";
   if (layered) {
    uint key; byte a; uint fl;
    if (GetLayeredWindowAttributes(h, out key, out a, out fl)) extra = " alpha=" + a + " flags=0x" + fl.ToString("X");
   }
   Console.WriteLine("hwnd=" + (long)h + " vis=" + IsWindowVisible(h) + " rect=(" + r.L + "," + r.T + "," + r.R + "," + r.B +
     ") ex=0x" + ex.ToString("X8") + " layered=" + layered + " NOREDIR=" + noredir + extra);
   return true;
  }, IntPtr.Zero);
 }
}
"@
[LayerProbe2]::Run()