@echo off
setlocal
rem Portable Carta: use only tools beside this launcher and writable local data.
set "CARTA_ROOT=%~dp0"
set "PATH=%CARTA_ROOT%git\cmd;%CARTA_ROOT%git\ucrt64\bin;%CARTA_ROOT%git\mingw64\bin;%CARTA_ROOT%tools;%PATH%"
set "XDG_DATA_HOME=%CARTA_ROOT%data"
if not exist "%XDG_DATA_HOME%" mkdir "%XDG_DATA_HOME%"
if errorlevel 1 (
  echo Cannot create Carta data directory. Extract the ZIP to a writable folder.
  pause
  exit /b 1
)
"%CARTA_ROOT%carta-gui.exe" %*
exit /b %ERRORLEVEL%
