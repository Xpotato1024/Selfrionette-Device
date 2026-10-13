@echo off
setlocal
cd /d "%~dp0"
if "%~1"=="" goto toolchain
if "%~1"=="dev" goto toolchain
if "%~1"=="build" goto toolchain
if "%~1"=="package" goto toolchain
echo Usage: run-native.cmd [dev^|build^|package]
exit /b 2
:toolchain
set "SRN_VS_ROOT=%SELFRIONETTE_VS_ROOT%"
if defined SRN_VS_ROOT goto activate
if defined VSCMD_VER goto ready
set "SRN_VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%SRN_VSWHERE%" goto missing
for /f "usebackq delims=" %%I in (`"%SRN_VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "SRN_VS_ROOT=%%I"
if not defined SRN_VS_ROOT goto missing
:activate
if not exist "%SRN_VS_ROOT%\Common7\Tools\VsDevCmd.bat" goto missing
call "%SRN_VS_ROOT%\Common7\Tools\VsDevCmd.bat" -no_logo -arch=x64 -host_arch=x64
if errorlevel 1 exit /b %errorlevel%
:ready
set "SRN_CARGO_BIN=%USERPROFILE%\.cargo\bin"
if defined CARGO_HOME set "SRN_CARGO_BIN=%CARGO_HOME%\bin"
set "PATH=%SRN_CARGO_BIN%;%PATH%"
if "%~1"=="build" goto build
if "%~1"=="package" goto package
call npm.cmd run dev
exit /b %errorlevel%
:build
call npm.cmd run build -- --debug
exit /b %errorlevel%
:package
call npm.cmd run package:windows
exit /b %errorlevel%
:missing
echo Developer build requires Visual Studio C++ Build Tools and Windows SDK.
echo Standard installations are detected with vswhere. For a custom verified installation,
echo set SELFRIONETTE_VS_ROOT to its installation directory in this terminal only.
echo To use the app without building, see docs/operations/device-manager-distribution.md.
exit /b 1
