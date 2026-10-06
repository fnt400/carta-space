#!/usr/bin/perl
use strict;
use warnings;
my ($prefix, $here, $traces) = @ARGV;
my (%opened, %external);
for my $trace (glob("$traces/packaging-latin.trace.*"), glob("$traces/packaging-unicode-fallback.trace.*")) {
    open my $fh, '<', $trace or die "$trace: $!";
    while (<$fh>) {
        next unless /openat\([^,]+, "([^"]+)".*\)\s*=\s*\d+/;
        my $path = $1;
        next unless -f $path;
        if (index($path, "$prefix/") == 0) { $opened{$path} = 1 }
        elsif ($path =~ m{^/(usr/(lib|share/perl|bin/perl)|lib|etc)/}) { $external{$path} = 1 }
    }
}
$opened{"$prefix/bin/$_"} = 1 for qw(groff preconv troff gropdf);
# All mounted custom fonts form the full configured PDF-only font closure.
$opened{$_} = 1 for glob "$prefix/share/groff/site-font/devpdf/*.pfa";
$opened{"$prefix/share/groff/site-font/devpdf/$_"} = 1 for qw(CR CI CB CM NS NH NA ND NJ download);
my (%bytes, %count);
print "GNU groff 1.24.1 PDF-only packaging measurement\n";
print "File sizes from stat (dereference metrics); no directory sizes, no stripping.\n";
print "CORE/CUSTOM records: tag<TAB>bytes<TAB>installed path.\n";
print "TRACE_DIRECTORY $traces\n";
for my $path (sort keys %opened) {
    my $tag = $path =~ m{/site-font/} ? 'CUSTOM' : 'CORE';
    my $size = -s $path;
    die "Missing $path" unless defined $size;
    $bytes{$tag} += $size;
    $count{$tag}++;
    print "$tag\t$size\t$path\n";
}
my $macro = -s "$here/carta.tmac";
print "CARTA_MACRO\t$macro\t$here/carta.tmac\n";
print "SUMMARY $count{CORE} core files / $bytes{CORE} bytes\n";
print "SUMMARY $count{CUSTOM} custom PDF font files / $bytes{CUSTOM} bytes\n";
print 'SUMMARY renderer + custom fonts + carta.tmac: ', $count{CORE} + $count{CUSTOM} + 1,
    ' files / ', $bytes{CORE} + $bytes{CUSTOM} + $macro, " bytes\n";
print "\nEXTERNAL observed opens (not a complete portable Perl/system closure):\n";
my ($perl_bytes, $perl_files) = (0, 0);
for my $path (sort keys %external) {
    my $size = -s $path;
    print "EXTERNAL\t$size\t$path\n";
    if ($path =~ m{^/usr/(lib/.*/perl(?:-base)?/|share/perl/)} && $path !~ /perllocal/) {
        $perl_bytes += $size; $perl_files++;
    }
}
my $interpreter = -s '/usr/bin/perl';
print "PERL_INTERPRETER /usr/bin/perl $interpreter bytes\n";
print "PERL_OBSERVED_MODULES $perl_files files / $perl_bytes bytes (not complete Perl distribution)\n";
my @libraries = sort grep { m{^/lib/[^/]+/lib[^/]+\.so(?:\.[^/]+)*$} } keys %external;
push @libraries, '/lib64/ld-linux-x86-64.so.2';
my $library_bytes = 0;
for my $path (@libraries) {
    my $size = -s $path;
    $library_bytes += $size;
    print "ELF_LIBRARY\t$size\t$path\n";
}
print 'ELF_LIBRARY_SUM ', scalar @libraries, " files / $library_bytes bytes (system ABI-specific)\n";
print "\nEXCLUSIONS: all .t42, font conversion tools, AFMs, docs, unused groff tools, shared TTFs.\n";
print "No files were deleted or modified in the installed tree.\n";
