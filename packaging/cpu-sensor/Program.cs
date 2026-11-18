using System;
using System.Diagnostics;
using System.IO;
using System.IO.Pipes;
using System.Security.Principal;
using System.Threading;
using LibreHardwareMonitor.Hardware;
using LibreHardwareMonitor.PawnIo;

internal static class Program
{
    private const int AdminRequired = -20001;
    private const int MissingDriver = -20002;
    private const int Unsupported = -20003;
    private const int MissingReading = -20004;
    private const string PipePrefix = "PerformanceGlass-";

    private static int Main(string[] args)
    {
        Guid token;
        if (args.Length != 1 || !args[0].StartsWith(PipePrefix, StringComparison.Ordinal) ||
            !Guid.TryParseExact(args[0].Substring(PipePrefix.Length), "N", out token))
            return 1;

        try
        {
            using (var pipe = new NamedPipeClientStream(".", args[0], PipeDirection.Out,
                       PipeOptions.None, TokenImpersonationLevel.Anonymous))
            {
                pipe.Connect(8000);
                ReadCpu(pipe);
            }
        }
        catch (IOException) { return 0; } // Parent closed the pipe.
        catch (TimeoutException) { return 1; }
        return 0;
    }

    private static void ReadCpu(Stream pipe)
    {
        if (!PawnIo.IsInstalled || PawnIo.Version < new Version(2, 0))
        {
            ReportUntilDisconnected(pipe, MissingDriver);
            return;
        }

        bool canElevate;
        using (var identity = WindowsIdentity.GetCurrent())
            canElevate = !new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);

        var computer = new Computer { IsCpuEnabled = true };
        bool opened = false;
        bool received = false;
        try
        {
            computer.Open();
            opened = true;
            for (int attempt = 0; ; ++attempt)
            {
                float temperature = Temperature(computer);
                if (!float.IsNaN(temperature))
                {
                    received = true;
                    Send(pipe, (int)Math.Round(temperature * 100));
                }
                else if (received)
                {
                    Send(pipe, MissingReading);
                }
                else if (attempt >= 2)
                {
                    break;
                }
                Thread.Sleep(1000);
            }
        }
        catch (UnauthorizedAccessException) { }
        finally
        {
            if (opened) computer.Close();
        }
        ReportUntilDisconnected(pipe, canElevate ? AdminRequired : Unsupported);
    }

    private static float Temperature(Computer computer)
    {
        int bestPriority = 0;
        float best = float.NaN;
        foreach (var hardware in computer.Hardware)
        {
            if (hardware.HardwareType != HardwareType.Cpu) continue;
            hardware.Update();
            foreach (var sensor in hardware.Sensors)
            {
                if (sensor.SensorType != SensorType.Temperature || !sensor.Value.HasValue) continue;
                float value = sensor.Value.Value;
                if (float.IsNaN(value) || value <= 0 || value > 150) continue;
                string name = sensor.Name;
                if (name.IndexOf("Distance to TjMax", StringComparison.OrdinalIgnoreCase) >= 0) continue;
                int priority = name.IndexOf("Tctl/Tdie", StringComparison.OrdinalIgnoreCase) >= 0 ? 3
                    : name.IndexOf("Package", StringComparison.OrdinalIgnoreCase) >= 0 ? 2
                    : name.IndexOf("Core", StringComparison.OrdinalIgnoreCase) >= 0 ? 1 : 0;
                if (float.IsNaN(best) || priority > bestPriority || (priority == bestPriority && value > best))
                {
                    bestPriority = priority;
                    best = value;
                }
            }
        }
        return best;
    }

    private static void ReportUntilDisconnected(Stream pipe, int status)
    {
        while (true)
        {
            Send(pipe, status);
            Thread.Sleep(1000);
        }
    }

    private static void Send(Stream pipe, int data)
    {
        byte[] packet = BitConverter.GetBytes(data);
        pipe.Write(packet, 0, packet.Length);
    }
}
