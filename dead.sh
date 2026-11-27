#!/bin/bash
suite=$(cd "$(dirname "$0")" && pwd) || exit 1
cd "$suite/../Baking Powder" || exit 1
export PATH="$HOME/.cargo/bin:$PATH"
merged="$HOME/Library/Caches/baking-powder-dead/merged"
if [ -z "$DEAD_HELD" ]; then
    mkdir -p "$merged" || exit 1
    DEAD_HELD=1 exec lockf -k "$merged.lock" "$suite/dead.sh"
fi
sdk_kit=$(sed -nE 's/^pub mod ([a-z_]+);/\1/p' sdk/src/lib.rs | grep -vxE 'dsp|spectrum' | tr '\n' ' ')
sdk_mods=$(sed -nE 's/^pub mod ([a-z_]+);/\1/p' sdk/src/lib.rs | paste -sd '|' -)
printf '%s\n' "Baking Powder — layering"
printf '%s\n\n' "kit beneath dsp, dsp beneath core and fx, project above those, audio and the views above project, mcp above the views, the app above all: a module reaches only its own layer and the layers beneath it"
layering=$(cat <<'PERL'
use strict;
use warnings;
my @layers = (
    [kit => [qw(arp base64 wav inflate png), split ' ', $ENV{SDK_KIT}], []],
    [dsp => [qw(dsp spectrum)], [qw(kit)]],
    [core => [qw(core)], [qw(kit dsp)]],
    [fx => [qw(fx)], [qw(kit dsp)]],
    [project => [qw(project theory midi flatten metronome history grid)], [qw(kit dsp core fx)]],
    [audio => [qw(audio record)], [qw(kit dsp core fx project)]],
    [view => [qw(browser roll ruler lanes rack palette)], [qw(kit dsp core fx project)]],
    [mcp => [qw(mcp)], [qw(kit project view)]],
);
my (%layer_of, %modules_of, %may);
my %macro = map { $_ => 1 } qw(bank choice choose field flag int params table);
for my $layer (@layers) {
    my ($name, $modules) = @$layer;
    $layer_of{$_} = $name for @$modules;
    $modules_of{$name} = $modules;
}
for my $layer (@layers) {
    my ($name, $modules, $beneath) = @$layer;
    $may{$name}{$_} = 1 for @$modules, map { @{$modules_of{$_}} } @$beneath;
}
sub leaves {
    my ($tree) = @_;
    $tree =~ s/^\s+|\s+$//g;
    $tree =~ s/\s*::\s*/::/g;
    return $tree unless $tree =~ /^(.*?)\{(.*)\}$/;
    my ($prefix, $inner) = ($1, $2);
    my @items;
    my $item = '';
    my $level = 0;
    for my $char (split //, $inner) {
        $level++ if $char eq '{';
        $level-- if $char eq '}';
        if ($char eq ',' && $level == 0) {
            push @items, $item;
            $item = '';
            next;
        }
        $item .= $char;
    }
    push @items, $item;
    return map { leaves("$prefix$_") } grep { /\S/ } @items;
}
while (my $file = <STDIN>) {
    chomp $file;
    my ($crate, $inside) = $file =~ m{^(bp|sdk|fx|coresynth)/src/(.*)\.rs$} or next;
    my @parts = grep { $_ ne 'mod' && $_ ne 'lib' } split m{/}, $inside;
    next unless @parts;
    my $layer = $crate eq 'fx' ? 'fx' : $crate eq 'coresynth' ? 'core' : $layer_of{$parts[0]} or next;
    my $depth = @parts;
    open my $in, '<', $file or die "$file: $!";
    my $text = do { local $/; <$in> };
    close $in;
    $text =~ s/\s+/ /g;
    my @paths;
    push @paths, leaves($1) while $text =~ /\buse\s+([^;]+);/g;
    (my $code = $text) =~ s/\buse\s+[^;]+;//g;
    push @paths, $1 while $code =~ /\b((?:crate|super|sdk|fx|coresynth)(?:::super)*::[A-Za-z_](?:[\w:]*\w)?)/g;
    for my $path (@paths) {
        $path =~ s/\s+as\s+\w+$//;
        my @seg = map { /(\w+)/ ? $1 : '' } split /\s*::\s*/, $path;
        next if @seg < 2;
        next if $seg[0] eq 'crate' && @seg == 2 && $macro{$seg[1]};
        my $reach;
        if ($seg[0] eq 'crate') {
            $reach = $crate eq 'fx' ? 'fx' : $crate eq 'coresynth' ? 'core' : $seg[1];
        } elsif ($seg[0] eq 'sdk') {
            $reach = $seg[1];
        } elsif ($seg[0] eq 'coresynth') {
            $reach = 'core';
        } elsif ($seg[0] eq 'fx') {
            $reach = 'fx';
        } elsif ($seg[0] eq 'super') {
            my $ups = 0;
            $ups++ while $ups < @seg && $seg[$ups] eq 'super';
            next if $ups < $depth;
            $reach = $crate eq 'fx' ? 'fx' : $crate eq 'coresynth' ? 'core' : ($seg[$ups] // '');
        } else {
            next;
        }
        next if $may{$layer}{$reach};
        print "$file: $path\n";
    }
}
PERL
)
reaching=$(find sdk/src fx/src coresynth/src bp/src -name '*.rs' | sort | SDK_KIT="$sdk_kit" perl -e "$layering" | sort -u)
if [ -n "$reaching" ]; then
    printf '%s\n\n' "$reaching"
    printf '%s\n\n' "$(printf '%s\n' "$reaching" | wc -l | tr -d ' ') reach past their layer: kit may use only kit; dsp kit and dsp; core kit, dsp and core; fx kit, dsp and fx; project those four and project; audio and the views project and beneath; mcp the views, project and kit; only the app reaches everything"
else
    printf '%s\n\n' "Every module keeps to its layer."
fi
reached=$(cat <<'PERL'
use strict;
use warnings;
my %known = map { chomp; $_ => 1 } <STDIN>;
my @queue = grep { m{^[^/]+/src/(?:lib|main)\.rs$} } sort keys %known;
my %seen;
while (defined(my $file = shift @queue)) {
    next if $seen{$file}++;
    open my $in, '<', $file or die "$file: $!";
    my $text = do { local $/; <$in> };
    close $in;
    my $inside = $file =~ m{^([^/]+/src)/(?:lib|main)\.rs$} ? $1 : $file =~ m{^(.*)/mod\.rs$} ? $1 : $file =~ s/\.rs$//r;
    while ($text =~ /^\s*(?:pub(?:\x28[^\x29]*\x29)?\s+)?mod\s+(\w+)\s*;/mg) {
        push @queue, grep { $known{$_} } "$inside/$1.rs", "$inside/$1/mod.rs";
    }
}
print "$_\n" for grep { !$seen{$_} } sort keys %known;
PERL
)
printf '%s\n' "Baking Powder — orphans"
printf '%s\n\n' "a source file no mod line reaches is never compiled, so clippy and the dead code check below never read it"
if ! orphans=$(find sdk/src fx/src coresynth/src bp/src mac/src -name '*.rs' | perl -e "$reached"); then
    printf '%s\n' "the walk of the mod lines could not run"
    exit 2
fi
if [ -n "$orphans" ]; then
    printf '%s\n\n' "$orphans"
    printf '%s\n\n' "$(printf '%s\n' "$orphans" | wc -l | tr -d ' ') never compiled: name each in its parent with a mod line, or delete it"
else
    printf '%s\n\n' "Every source file is reached by a mod line."
fi
printf '%s\n' "Baking Powder — dead code"
printf '%s\n\n' "rustc's dead_code lint with sdk, fx, coresynth, bp and mac read as one program (apart, their pub items all count as exported, so cargo never checks them)"
rm -rf "$merged/src" && mkdir -p "$merged/src/shell" "$merged/src/fx" "$merged/src/core" || exit 1
printf '[package]\nname = "merged"\nversion = "0.1.0"\nedition = "2024"\n\n[[bin]]\nname = "merged"\npath = "src/main.rs"\n\n[workspace]\n' > "$merged/Cargo.toml"
cp -R sdk/src/. "$merged/src/" && rm "$merged/src/lib.rs"
cp -R bp/src/. "$merged/src/" && rm "$merged/src/lib.rs"
cp -R fx/src/. "$merged/src/fx/" && mv "$merged/src/fx/lib.rs" "$merged/src/fx/mod.rs"
cp -R coresynth/src/. "$merged/src/core/" && mv "$merged/src/core/lib.rs" "$merged/src/core/mod.rs"
{ perl -pe 's/^pub use sdk::(\w+);/mod $1;/; s/^pub use fx;/mod fx;/; s/^pub use coresynth as core;/mod core;/' bp/src/lib.rs; printf 'mod shell;\n\nfn main() {\n    shell::run();\n}\n'; } > "$merged/src/main.rs"
for f in mac/src/*.rs; do
    case "$(basename "$f")" in
        main.rs) ;;
        lib.rs) cp "$f" "$merged/src/shell/mod.rs" ;;
        *) cp "$f" "$merged/src/shell/" ;;
    esac
done
perl -pi -e 's/\bcrate::/crate::shell::/g; s/\bbp::/crate::/g' "$merged/src/shell/"*.rs
find "$merged/src/fx" -name '*.rs' -print0 | xargs -0 perl -pi -e 's/(?<!\$)\bcrate::/crate::fx::/g; s/\bsdk::/crate::/g'
find "$merged/src/core" -name '*.rs' -print0 | xargs -0 perl -pi -e 's/(?<!\$)\bcrate::/crate::core::/g; s/\bsdk::/crate::/g'
find "$merged/src" \( -path "$merged/src/fx" -o -path "$merged/src/core" -o -path "$merged/src/shell" \) -prune -o -name '*.rs' -print0 | xargs -0 perl -pi -e 's/\bsdk::/crate::/g; s/\bcoresynth::/crate::core::/g; s/(?<![\w:])fx::/crate::fx::/g'
find "$merged/src" -name '*.rs' -print0 | xargs -0 perl -pi -e 's/^(\s*)pub mod /$1pub(crate) mod /'
output=$(cd "$merged" && cargo check --quiet --message-format=short 2>&1)
status=$?
findings=$(printf '%s\n' "$output" | grep -E ': (warning|error)' | SDK_MODS="$sdk_mods" perl -pe 's#^src/shell/mod\.rs#mac/src/lib.rs#; s#^src/shell/#mac/src/#; s#^src/main\.rs#bp/src/lib.rs#; s#^src/fx/mod\.rs#fx/src/lib.rs#; s#^src/fx/#fx/src/#; s#^src/core/mod\.rs#coresynth/src/lib.rs#; s#^src/core/#coresynth/src/#; s#^src/($ENV{SDK_MODS})(/|\.rs)#sdk/src/$1$2#; s#^src/#bp/src/#')
if [ "$status" -ne 0 ] && ! printf '%s\n' "$findings" | grep -q ': error'; then
    printf '%s\n' "cargo check could not run:" "$output"
    exit 2
fi
if [ -z "$findings" ]; then
    printf '%s\n' "Nothing is dead: every item is reachable from the shell's main."
    [ -n "$reaching$orphans" ] && exit 1
    exit 0
fi
printf '%s\n\n' "$findings"
printf '%s\n' "$(printf '%s\n' "$findings" | wc -l | tr -d ' ') found — rustc cannot see a flag that is read but never set; follow any never-called setter by hand."
exit 1
