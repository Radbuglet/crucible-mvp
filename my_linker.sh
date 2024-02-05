args=("$@")
for ((i=0; i<"${#args[@]}"; ++i)); do
    case ${args[i]} in
        --gc-sections) unset args[i]; unset args[i+1]; break;;
    esac
done

$(rustc --print=sysroot)/lib/rustlib/$(rustc -vV | awk '/host:/ {print $2}')/bin/rust-lld "${args[@]}" -r
