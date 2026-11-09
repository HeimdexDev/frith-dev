import Frith.Model

open Lean Frith

def main : IO UInt32 := do
  let stdin ← IO.getStdin
  let stdout ← IO.getStdout
  let stderr ← IO.getStderr
  let line ← stdin.getLine
  let parsed := Json.parse line >>= fromJson? (α := Input)
  match parsed with
  | .error _ =>
      stderr.putStrLn "invalid checker input"
      return 2
  | .ok input =>
      if input.protocolVersion != 1 then
        stderr.putStrLn "unsupported protocol"
        return 2
      stdout.putStrLn (toJson (step input.state input.command input.context)).compress
      return 0
