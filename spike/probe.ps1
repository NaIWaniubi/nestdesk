Add-Type @"
using System;using System.Text;using System.Runtime.InteropServices;
public class WinProbe {
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
 public delegate bool EnumProc(IntPtr h, IntPtr l);
 [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out int v, int s);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
 public static void Run() {
  EnumWindows((h, l) => {
   var sb = new StringBuilder(64); GetClassName(h, sb, 64);
   if (sb.ToString() == "NestDeskSpike") {
    RECT r; GetWindowRect(h, out r);
    int cloak = -1; DwmGetWindowAttribute(h, 14, out cloak, 4);
    Console.WriteLine("hwnd=" + h + " vis=" + IsWindowVisible(h) + " iconic=" + IsIconic(h) + " rect=(" + r.L + "," + r.T + "," + r.R + "," + r.B + ") cloaked=" + cloak);
   }
   return true;
  }, IntPtr.Zero);
 }
}
"@
[WinProbe]::Run()
