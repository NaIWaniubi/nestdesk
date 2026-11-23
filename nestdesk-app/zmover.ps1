param([string]$Mode) # top | bottom
Add-Type @"
using System;using System.Runtime.InteropServices;
public class ZMover {
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
 public delegate bool EnumProc(IntPtr h, IntPtr l);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [DllImport("user32.dll", EntryPoint="SetWindowPos")] public static extern bool SWP(IntPtr h, IntPtr a, int x,int y,int cx,int cy, uint f);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
 const uint NOMOVE=0x2, NOSIZE=0x1, NOACTIVATE=0x10, FRAMECHANGED=0x20;
 public static void Move(string mode) {
  uint target = 0;
  foreach (var p in System.Diagnostics.Process.GetProcessesByName("nestdesk-app")) { target=(uint)p.Id; break; }
  if (target==0) { Console.WriteLine("no proc"); return; }
  IntPtr after = (mode=="topmost") ? new IntPtr(-1) : new IntPtr(1);
  int n=0;
  EnumWindows((h,l)=>{
   uint pid; GetWindowThreadProcessId(h,out pid);
   if (pid!=target) return true;
   RECT r; GetWindowRect(h,out r);
   if (r.R-r.L<60) return true;
   if (!IsWindowVisible(h)) return true;
   SWP(h, after, 0,0,0,0, NOMOVE|NOSIZE|NOACTIVATE|FRAMECHANGED);
   Console.WriteLine("moved("+mode+"): "+(long)h+" ("+r.L+","+r.T+","+r.R+","+r.B+")");
   n++; return true;
  }, IntPtr.Zero);
  Console.WriteLine("count="+n);
 }
}
"@
[ZMover]::Move($Mode)