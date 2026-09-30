@echo off
REM AeroPulse-NG Windows USB driver helper (RTL-SDR + AWOS serial).
REM Requires Zadig (https://zadig.akeo.ie) for RTL2832U; AWOS adapters use built-in usbser.
echo AeroPulse-NG USB setup
echo ======================
echo 1. RTL-SDR 1090 MHz (RTL2832U, VID 0BDA PID 2838):
echo    Open Zadig -^> Options -^> List All Devices -^> Bulk-In Interface 0 -^> WinUSB -^> Install Driver
echo 2. AWOS RS-485 mast (FTDI/CP210x):
echo    Install vendor VCP driver if COM port missing, then note COM port for --serial.
echo 3. Verify: python -m serial.tools.list_ports
pause
