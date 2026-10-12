@echo off
setlocal
set "SRN_VS_ROOT=%SELFRIONETTE_VS_ROOT%"
if not defined SRN_VS_ROOT set "SRN_VS_ROOT=D:\Xpotato-apps\toolchains\vs2022-msvc"
if not exist "%SRN_VS_ROOT%\Common7\Tools\VsDevCmd.bat" (
  echo Registered or verified VS toolchain path is required. Set SELFRIONETTE_VS_ROOT.
  exit /b 1
)
call "%SRN_VS_ROOT%\Common7\Tools\VsDevCmd.bat" -no_logo -arch=x64 -host_arch=x64
if errorlevel 1 exit /b %errorlevel%
set "SRN_CARGO_BIN=%USERPROFILE%\.cargo\bin"
if defined CARGO_HOME set "SRN_CARGO_BIN=%CARGO_HOME%\bin"
set "PATH=%SRN_CARGO_BIN%;%PATH%"
cd /d "%~dp0"
if "%~1"=="build" goto build
if not "%~1"=="" if not "%~1"=="dev" (
  echo Usage: run-native.cmd [build^|dev]
  exit /b 2
)
call npm.cmd run dev
exit /b %errorlevel%
:build
call npm.cmd run build -- --debug
exit /b %errorlevel%
