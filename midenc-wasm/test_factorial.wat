(module
  ;; Factorial function (iterative)
  (func $factorial (param $n i32) (result i32)
    (local $result i32)
    (local $i i32)

    ;; Initialize result to 1
    i32.const 1
    local.set $result

    ;; Initialize i to 1
    i32.const 1
    local.set $i

    ;; Loop while i <= n
    (block $break
      (loop $continue
        ;; Check if i > n
        local.get $i
        local.get $n
        i32.gt_s
        br_if $break

        ;; result = result * i
        local.get $result
        local.get $i
        i32.mul
        local.set $result

        ;; i++
        local.get $i
        i32.const 1
        i32.add
        local.set $i

        br $continue
      )
    )

    local.get $result
  )
  (export "factorial" (func $factorial))
)
