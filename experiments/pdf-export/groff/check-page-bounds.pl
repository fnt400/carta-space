#!/usr/bin/perl
use strict;
use warnings;
my $limit = 272 * 72 / 25.4;
for my $file (@ARGV) {
    open my $fh, '<', $file or die "$file: $!";
    local $/;
    my $xml = <$fh>;
    my $page = 0;
    while ($xml =~ /<page\b[^>]*>(.*?)<\/page>/sg) {
        my $content = $1;
        $page++;
        my ($maximum, $footers, $words) = (0, 0, 0);
        while ($content =~ /<word\b([^>]*)>(.*?)<\/word>/sg) {
            my ($attributes, $text) = ($1, $2);
            my %box = $attributes =~ /(\w+)="([^"]+)"/g;
            my $center = ($box{xMin} + $box{xMax}) / 2;
            if ($text eq "$page" && $box{yMin} > $limit && abs($center - 297.637) < 1) {
                $footers++;
                next;
            }
            $words++;
            $maximum = $box{yMax} if $box{yMax} > $maximum;
            die "$file page $page: body '$text' below 272 mm ($box{yMax} pt)\n"
                if $box{yMax} > $limit;
        }
        die "$file page $page: expected one centered footer, got $footers\n" unless $footers == 1;
        printf "%s page %d: PASS %d body words, max yMax %.3f pt / %.3f mm; one centered footer\n",
            $file, $page, $words, $maximum, $maximum * 25.4 / 72;
    }
    die "$file: no pages\n" unless $page;
}
