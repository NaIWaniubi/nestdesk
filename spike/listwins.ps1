Add-Type @"
using System;using System.Text;using System.Runtime.InteropServices;
public class WinList {
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
 public delegate bool EnumProc(IntPtr h, IntPtr l);
 [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
 [DllImport("user32.dll")] public static extern int GetWindowTextLengthW(IntPtr h);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [DllImport("user32.dll")] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
 public static void Run() {
  EnumWindows((h, l) => {
   if (!IsWindowVisible(h)) return true;
   int len = GetWindowTextLengthW(h); if (len == 0) return true;
   var sb = new StringBuilder(len + 2); GetWindowTextW(h, sb, sb.Capacity);
   var cn = new StringBuilder(64); GetClassNameW(h, cn, 64);
   RECT r; GetWindowRect(h, out r);
   if (r.R - r.L < 80 || r.B - r.T < 40) return true;
   Console.WriteLine(r.L + "," + r.T + "," + r.R + "," + r.B + " | " + cn + " | " + sb);
   return true;
  }, IntPtr.Zero);
 }
}
"@
[WinList]::Run()
