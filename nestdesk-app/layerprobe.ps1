Add-Type @"
using System;using System.Runtime.InteropServices;
public class LayerProbe {
 [DllImport("user32.dll", SetLastError=true)] public static extern int GetWindowLong(IntPtr h, int i);
 [DllImport("user32.dll")] public static extern bool GetLayeredWindowAttributes(IntPtr h, out uint key, out byte alpha, out uint flags);
 [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
 public static void Run(long[] hwnds) {
  foreach (var hv in hwnds) {
   IntPtr h = new IntPtr(hv);
   int ex = GetWindowLong(h, -20);
   bool layered = (ex & 0x80000) != 0;
   string extra = "";
   if (layered) {
    uint key; byte a; uint fl;
    if (GetLayeredWindowAttributes(h, out key, out a, out fl)) extra = " alpha=" + a + " flags=" + fl;
    else extra = " layered-but-no-attrs(err=" + Marshal.GetLastWin32Error() + ")";
   }
   Console.WriteLine("hwnd=" + hv + " exstyle=0x" + ex.ToString("X8") + " layered=" + layered + extra);
  }
 }
}
"@
[LayerProbe]::Run(@(22611556, 41290900, 21042194, 7215046))
